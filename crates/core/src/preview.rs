//! Converts decoded pictures to RGBA for the live preview, on its own thread.
//!
//! The decoding thread hands over each picture (a reference, no copy) and goes
//! on decoding; this thread converts only the most recent one, scaled to the
//! preview size. On a slow PC the conversion no longer slows the decoding down
//! (which made the stream pile up: growing latency), and pictures replaced
//! before the screen could show them are never converted.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

use crate::api::{FrameSlot, Notifier};
use crate::decode::{DecodedFrame, RgbaConverter};

/// Longest side of the preview picture: more than a screen shows for a
/// phone (the recording keeps the full resolution).
pub const PREVIEW_MAX_SIDE: u32 = 1600;

/// Delay of the preview beyond the best seen at which decoding skips to the
/// next key frame (one per second) to catch up.
pub const MAX_PREVIEW_LAG_US: i64 = 300_000;

/// Keeps the preview close to live when the PC cannot decode every picture
/// in time (high resolution on a slow PC).
///
/// Each packet's arrival time minus its device time is its delay; the
/// smallest delay seen is the network's own. When a packet arrives much later
/// than that, the PC fell behind: pictures are skipped until the next key
/// frame (the only ones that can start a picture again). Recording is not
/// affected: it gets every packet before this point.
#[derive(Debug, Default)]
pub struct LagGuard {
    best_us: Option<i64>,
    skipping: bool,
    /// Packets skipped so far, for the log.
    pub skipped: u64,
}

impl LagGuard {
    /// New device clock (rotation): delays are measured again.
    pub fn reset(&mut self) {
        self.best_us = None;
        self.skipping = false;
    }

    /// Should this packet be decoded? `arrival_us`: when it was read, on any
    /// clock that does not jump (µs).
    pub fn decode(&mut self, pts_us: u64, arrival_us: i64, key_frame: bool) -> bool {
        let delay = arrival_us - i64::try_from(pts_us).unwrap_or(i64::MAX);
        let best = self.best_us.map_or(delay, |b| b.min(delay));
        self.best_us = Some(best);
        if self.skipping {
            if key_frame {
                self.skipping = false;
                return true;
            }
            self.skipped += 1;
            return false;
        }
        if delay - best > MAX_PREVIEW_LAG_US && !key_frame {
            tracing::info!(
                late_ms = (delay - best) / 1000,
                "preview behind: skipping to the next key frame"
            );
            self.skipping = true;
            self.skipped += 1;
            return false;
        }
        true
    }
}

#[derive(Default)]
struct Shared {
    pending: Mutex<Option<DecodedFrame>>,
    wake: Condvar,
    stop: AtomicBool,
}

fn lock(shared: &Shared) -> MutexGuard<'_, Option<DecodedFrame>> {
    shared
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Handle on the conversion thread; stops it when dropped.
pub struct PreviewConverter {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl PreviewConverter {
    pub fn start(frames: Arc<FrameSlot>, notify: Notifier) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let worker = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("preview-convert".into())
            .spawn(move || run(&worker, &frames, &notify))?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }

    /// Hands over the newest picture. Returns `true` when the previous one was
    /// replaced before being converted (it never reached the screen).
    pub fn push(&self, frame: DecodedFrame) -> bool {
        let replaced = lock(&self.shared).replace(frame).is_some();
        self.shared.wake.notify_one();
        replaced
    }
}

impl Drop for PreviewConverter {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.wake.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(shared: &Shared, frames: &FrameSlot, notify: &Notifier) {
    let mut converter = RgbaConverter::default();
    loop {
        let decoded = {
            let mut pending = lock(shared);
            loop {
                if shared.stop.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(frame) = pending.take() {
                    break frame;
                }
                pending = shared
                    .wake
                    .wait(pending)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        match converter.convert_fit(&decoded.frame, decoded.pts_us, PREVIEW_MAX_SIDE) {
            Ok(frame) => {
                frames.put(frame);
                notify();
            }
            Err(err) => tracing::debug!(%err, "preview conversion failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 60 fps device timestamps.
    const FRAME: u64 = 16_667;

    #[test]
    fn on_time_pictures_are_all_decoded() {
        let mut guard = LagGuard::default();
        for i in 0..120u64 {
            // Network jitter of a few ms is not lag.
            let jitter = i64::try_from(i % 3).unwrap() * 4_000;
            let arrival = i64::try_from(i * FRAME).unwrap() + 50_000 + jitter;
            assert!(guard.decode(i * FRAME, arrival, i % 60 == 0));
        }
        assert_eq!(guard.skipped, 0);
    }

    #[test]
    fn falling_behind_skips_to_the_next_key_frame() {
        let mut guard = LagGuard::default();
        assert!(guard.decode(0, 0, true));
        // The PC takes 25 ms per picture: each packet is read later and later.
        let mut decoded = 0;
        for i in 1..=60u64 {
            let arrival = i64::try_from(i * 25_000).unwrap();
            if guard.decode(i * FRAME, arrival, false) {
                decoded += 1;
            }
        }
        assert!(guard.skipped > 0, "it caught up");
        assert!(decoded < 60);
        // The next key frame is decoded and ends the skipping.
        assert!(guard.decode(61 * FRAME, 61 * 17_000, true));
        assert!(
            guard.decode(62 * FRAME, 62 * 17_000, false),
            "back to normal"
        );
    }

    #[test]
    fn fit_size_scales_the_long_side() {
        use crate::decode::fit_size;
        assert_eq!(fit_size(1220, 2712, 1600), (718, 1600));
        assert_eq!(fit_size(862, 1920, 1600), (718, 1600));
        assert_eq!(fit_size(720, 1280, 1600), (720, 1280), "never upscaled");
        assert_eq!(fit_size(2712, 1220, 1600), (1600, 718), "landscape");
    }
}
