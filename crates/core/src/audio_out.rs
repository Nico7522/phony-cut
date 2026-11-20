//! Plays the phone's sound on the PC's default output device (`cpal`).
//!
//! Decoded samples are pushed into a small queue that the device callback
//! drains. The queue absorbs network jitter without adding much delay:
//! playback starts once [`PREFILL`] is buffered (and again after running dry),
//! and when more than [`MAX_LATENCY`] piles up (the PC fell behind), the
//! oldest samples are dropped back down to [`TARGET_LATENCY`] so the sound
//! stays in sync with the picture.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const PREFILL: Duration = Duration::from_millis(60);
const TARGET_LATENCY: Duration = Duration::from_millis(80);
const MAX_LATENCY: Duration = Duration::from_millis(200);

#[derive(Debug, thiserror::Error)]
pub enum AudioOutputError {
    #[error("no sound output device")]
    NoDevice,
    #[error("sound output: {0}")]
    Device(String),
    #[error("could not start the sound thread: {0}")]
    Thread(#[from] std::io::Error),
}

/// Format of the opened device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputFormat {
    pub rate: u32,
    pub channels: u16,
}

impl OutputFormat {
    /// Number of interleaved values for `duration`.
    fn values(self, duration: Duration) -> usize {
        let frames = u128::from(self.rate) * duration.as_micros() / 1_000_000;
        usize::try_from(frames).unwrap_or(usize::MAX) * usize::from(self.channels)
    }
}

/// Sample queue shared with the device callback.
#[derive(Debug)]
struct Queue {
    samples: VecDeque<f32>,
    /// Playing (enough was buffered); false after running dry.
    primed: bool,
    prefill: usize,
    target: usize,
    max: usize,
    channels: usize,
    /// Samples dropped to catch up, for the log.
    dropped: u64,
}

impl Queue {
    fn new(format: OutputFormat) -> Self {
        Self {
            samples: VecDeque::new(),
            primed: false,
            prefill: format.values(PREFILL),
            target: format.values(TARGET_LATENCY),
            max: format.values(MAX_LATENCY),
            channels: usize::from(format.channels.max(1)),
            dropped: 0,
        }
    }

    fn push(&mut self, samples: &[f32]) {
        self.samples.extend(samples);
        if self.samples.len() > self.max {
            // Keep whole frames (all channels of one instant) together.
            let excess = self.samples.len() - self.target;
            let excess = excess - excess % self.channels;
            self.samples.drain(..excess);
            self.dropped += excess as u64;
        }
    }

    /// Fills the device buffer; silence while (re)buffering.
    fn fill<T: cpal::Sample + cpal::FromSample<f32>>(&mut self, out: &mut [T]) {
        if !self.primed && self.samples.len() >= self.prefill {
            self.primed = true;
        }
        for value in out.iter_mut() {
            let sample = if self.primed {
                match self.samples.pop_front() {
                    Some(sample) => sample,
                    None => {
                        self.primed = false;
                        0.0
                    }
                }
            } else {
                0.0
            };
            *value = T::from_sample(sample);
        }
    }
}

fn lock(queue: &Mutex<Queue>) -> MutexGuard<'_, Queue> {
    queue.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The PC's sound output, open while this value lives.
pub struct AudioOutput {
    format: OutputFormat,
    queue: Arc<Mutex<Queue>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AudioOutput {
    /// Opens the default output device. The stream lives on its own thread
    /// (cpal streams are not `Send` on every platform).
    pub fn start() -> Result<Self, AudioOutputError> {
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("audio-out".into())
            .spawn(move || run(&thread_stop, &ready_tx))?;
        match ready_rx.recv() {
            Ok(Ok((format, queue))) => {
                tracing::info!(
                    rate = format.rate,
                    channels = format.channels,
                    "sound output open"
                );
                Ok(Self {
                    format,
                    queue,
                    stop,
                    thread: Some(thread),
                })
            }
            Ok(Err(err)) => {
                let _ = thread.join();
                Err(err)
            }
            Err(_) => Err(AudioOutputError::Device("sound thread ended".into())),
        }
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    /// Queues interleaved samples in the device format.
    pub fn push(&self, samples: &[f32]) {
        lock(&self.queue).push(samples);
    }

    /// Drops what is queued (pause, seek): silence until new samples come.
    pub fn clear(&self) {
        let mut queue = lock(&self.queue);
        queue.samples.clear();
        queue.primed = false;
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let dropped = lock(&self.queue).dropped;
        if dropped > 0 {
            tracing::info!(
                dropped_ms = dropped * 1000
                    / (u64::from(self.format.rate) * u64::from(self.format.channels)).max(1),
                "sound output closed (late sound dropped to stay in sync)"
            );
        }
    }
}

type Ready = Result<(OutputFormat, Arc<Mutex<Queue>>), AudioOutputError>;

fn run(stop: &AtomicBool, ready: &crossbeam_channel::Sender<Ready>) {
    let opened = open();
    let stream = match opened {
        Ok((stream, format, queue)) => {
            let _ = ready.send(Ok((format, queue)));
            stream
        }
        Err(err) => {
            let _ = ready.send(Err(err));
            return;
        }
    };
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(50));
    }
    drop(stream);
}

fn open() -> Result<(cpal::Stream, OutputFormat, Arc<Mutex<Queue>>), AudioOutputError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or(AudioOutputError::NoDevice)?;
    let supported = device
        .default_output_config()
        .map_err(|e| AudioOutputError::Device(e.to_string()))?;
    let config = supported.config();
    let format = OutputFormat {
        rate: config.sample_rate,
        channels: config.channels,
    };
    let queue = Arc::new(Mutex::new(Queue::new(format)));
    let on_error = |err: cpal::Error| tracing::warn!(%err, "sound output error");
    let stream = match supported.sample_format() {
        cpal::SampleFormat::I16 => build::<i16>(&device, config, &queue, on_error),
        cpal::SampleFormat::U16 => build::<u16>(&device, config, &queue, on_error),
        cpal::SampleFormat::I32 => build::<i32>(&device, config, &queue, on_error),
        _ => build::<f32>(&device, config, &queue, on_error),
    }
    .map_err(|e| AudioOutputError::Device(e.to_string()))?;
    stream
        .play()
        .map_err(|e| AudioOutputError::Device(e.to_string()))?;
    Ok((stream, format, queue))
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    queue: &Arc<Mutex<Queue>>,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let queue = Arc::clone(queue);
    device.build_output_stream::<T, _, _>(
        config,
        move |out: &mut [T], _| lock(&queue).fill(out),
        on_error,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue() -> Queue {
        // 1 kHz mono: 1 value per millisecond, easy to reason about.
        Queue::new(OutputFormat {
            rate: 1000,
            channels: 1,
        })
    }

    #[test]
    fn silence_until_prefilled() {
        let mut q = queue();
        q.push(&[1.0; 30]);
        let mut out = [9.0f32; 10];
        q.fill(&mut out);
        assert_eq!(out, [0.0; 10], "30 ms < 60 ms prefill");
        q.push(&[1.0; 40]);
        q.fill(&mut out);
        assert_eq!(out, [1.0; 10]);
    }

    #[test]
    fn rebuffers_after_running_dry() {
        let mut q = queue();
        q.push(&[1.0; 60]);
        let mut out = [0.0f32; 70];
        q.fill(&mut out);
        assert_eq!(out[59], 1.0);
        assert_eq!(out[60], 0.0, "ran dry: silence");
        q.push(&[1.0; 10]);
        let mut next = [9.0f32; 5];
        q.fill(&mut next);
        assert_eq!(next, [0.0; 5], "waits for the prefill again");
    }

    #[test]
    fn late_sound_is_dropped_down_to_the_target() {
        let mut q = queue();
        q.push(&[0.5; 250]);
        assert_eq!(q.samples.len(), 80, "back to 80 ms");
        assert_eq!(q.dropped, 170);
    }

    #[test]
    fn drop_keeps_channel_frames_together() {
        let mut q = Queue::new(OutputFormat {
            rate: 1000,
            channels: 2,
        });
        let samples: Vec<f32> = (0..500).map(|i| (i % 2) as f32).collect();
        q.push(&samples);
        assert_eq!(q.samples.len() % 2, 0);
        assert_eq!(q.samples.front(), Some(&0.0), "starts on a left sample");
    }
}
