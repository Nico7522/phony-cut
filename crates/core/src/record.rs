//! Recording: the H.264 packets received from the phone are written as-is
//! (no re-encoding) into a Matroska (.mkv) file.
//!
//! - Writing starts on a key frame (requested every second from the server).
//! - Timestamps come from the device PTS, rebased to 0, and stay monotonic
//!   even if the device clock restarts (encoder reset on rotation).
//! - Each packet is written one packet late, so its duration is known; on stop
//!   the last frame lasts until the stop instant, so a static screen at the end
//!   is not cut from the file duration.
//! - Crash robustness: clusters of at most 1 s, flushed to disk as written. A
//!   crash loses ~1 s and the index; the file still plays.

use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use ffmpeg::util::mathematics::Rescale;
use ffmpeg::{codec, format, Dictionary, Rational};
use ffmpeg_next as ffmpeg;

use crate::api::{
    Event, Notifier, RecordingEnd, RecordingEntry, RecordingInfo, RecordingProblem, RecordingState,
};
use crate::markers::{sidecar_path, CutFile, Marker};
use crate::platform::LocalDateTime;
use crate::scrcpy::protocol::MediaPacket;

/// How often progress is reported while recording.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
/// Device and arrival clocks may disagree by this much before the device
/// timestamps are considered discontinuous.
const MAX_CLOCK_DRIFT_US: i64 = 1_000_000;
/// Required zeroed padding after FFmpeg extradata (`AV_INPUT_BUFFER_PADDING_SIZE`).
const FFMPEG_PADDING: usize = 64;
const MICROS: Rational = Rational(1, 1_000_000);

/// `PhoneCut 2026-11-17 16-05-32.mkv`
pub fn recording_file_name(at: LocalDateTime) -> String {
    format!(
        "PhoneCut {:04}-{:02}-{:02} {:02}-{:02}-{:02}.mkv",
        at.year, at.month, at.day, at.hour, at.minute, at.second
    )
}

/// `folder/name`, or `folder/stem (2).ext`… if it already exists.
pub fn unique_path(folder: &Path, name: &str) -> PathBuf {
    let candidate = folder.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    (2..)
        .map(|n| {
            let file = match &ext {
                Some(ext) => format!("{stem} ({n}).{ext}"),
                None => format!("{stem} ({n})"),
            };
            folder.join(file)
        })
        .find(|p| !p.exists())
        .unwrap_or(candidate)
}

/// Recordings (`.mkv`) of a folder, newest first. A missing folder is empty.
pub fn list_recordings(folder: &Path) -> Vec<RecordingEntry> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut list: Vec<RecordingEntry> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mkv")))
        .filter_map(|path| {
            let meta = fs::metadata(&path).ok()?;
            let cuts = CutFile::load(&path).map_or(0, |c| c.markers.len());
            Some(RecordingEntry {
                bytes: meta.len(),
                modified: meta.modified().ok(),
                cuts,
                path,
            })
        })
        .collect();
    list.sort_by_key(|entry| std::cmp::Reverse(entry.modified));
    list
}

/// Maps device PTS to file time (µs from the first recorded frame).
#[derive(Debug, Clone)]
pub struct Timeline {
    last_in: u64,
    last_out_us: i64,
    last_arrival: Instant,
}

impl Timeline {
    /// The first frame (device time `first_pts`) is file time 0.
    pub fn new(first_pts: u64, arrival: Instant) -> Self {
        Self {
            last_in: first_pts,
            last_out_us: 0,
            last_arrival: arrival,
        }
    }

    /// File time of a frame. Uses the device clock while it is consistent with
    /// arrival times; falls back to arrival times across discontinuities.
    pub fn map(&mut self, pts: u64, arrival: Instant) -> i64 {
        let wall = micros(arrival.saturating_duration_since(self.last_arrival));
        let device = i64::try_from(pts).unwrap_or(i64::MAX)
            - i64::try_from(self.last_in).unwrap_or(i64::MAX);
        let delta = if device >= 0 && (device - wall).abs() <= MAX_CLOCK_DRIFT_US {
            device
        } else {
            tracing::info!(device, wall, "device timestamps jumped, using arrival time");
            wall
        };
        self.last_out_us += delta.max(0);
        self.last_in = pts;
        self.last_arrival = arrival;
        self.last_out_us
    }

    /// File time corresponding to `at` (after the last frame).
    pub fn time_at(&self, at: Instant) -> i64 {
        self.last_out_us + micros(at.saturating_duration_since(self.last_arrival))
    }
}

fn micros(d: Duration) -> i64 {
    i64::try_from(d.as_micros()).unwrap_or(i64::MAX)
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("FFmpeg: {0}")]
    Ffmpeg(#[from] ffmpeg::Error),
    #[error("out of memory")]
    OutOfMemory,
}

struct Pending {
    data: Vec<u8>,
    time_us: i64,
    key_frame: bool,
}

/// Writes one H.264 video track into a Matroska file.
pub struct MkvWriter {
    output: format::context::Output,
    time_base: Rational,
    pending: Option<Pending>,
    last_ts: Option<i64>,
}

impl MkvWriter {
    /// Creates the file. `config` is the codec configuration packet (SPS/PPS,
    /// Annex-B) as sent by the server.
    pub fn create(path: &Path, width: u32, height: u32, config: &[u8]) -> Result<Self, WriteError> {
        crate::decode::init_ffmpeg();
        let mut output = format::output_as(path, "matroska")?;
        {
            let mut stream = output.add_stream(Option::<ffmpeg::Codec>::None)?;
            stream.set_time_base((1, 1000));
            // SAFETY: `stream` was just created by avformat_new_stream and is
            // owned by `output`; its `codecpar` is valid and allocated. FFmpeg
            // requires extradata to be av_malloc'ed with zeroed padding, and
            // frees it with the stream.
            unsafe {
                let par = (*stream.as_mut_ptr()).codecpar;
                (*par).codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
                (*par).codec_id = ffmpeg::ffi::AVCodecID::AV_CODEC_ID_H264;
                (*par).width = i32::try_from(width).unwrap_or(i32::MAX);
                (*par).height = i32::try_from(height).unwrap_or(i32::MAX);
                let extradata = ffmpeg::ffi::av_mallocz(config.len() + FFMPEG_PADDING).cast::<u8>();
                if extradata.is_null() {
                    return Err(WriteError::OutOfMemory);
                }
                ptr::copy_nonoverlapping(config.as_ptr(), extradata, config.len());
                (*par).extradata = extradata;
                (*par).extradata_size = i32::try_from(config.len()).unwrap_or(0);
            }
        }
        let mut options = Dictionary::new();
        // Small clusters + flushing: a crash loses at most ~1 s of video.
        options.set("cluster_time_limit", "1000");
        options.set("flush_packets", "1");
        output.write_header_with(options)?;
        let time_base = output
            .stream(0)
            .map(|s| s.time_base())
            .ok_or(ffmpeg::Error::StreamNotFound)?;
        Ok(Self {
            output,
            time_base,
            pending: None,
            last_ts: None,
        })
    }

    /// Queues a frame at file time `time_us`; the previous one is written now
    /// that its duration is known.
    pub fn write(
        &mut self,
        data: Vec<u8>,
        time_us: i64,
        key_frame: bool,
    ) -> Result<(), WriteError> {
        if let Some(previous) = self.pending.take() {
            let duration = time_us - previous.time_us;
            self.write_packet(previous, duration)?;
        }
        self.pending = Some(Pending {
            data,
            time_us,
            key_frame,
        });
        Ok(())
    }

    /// Writes the last frame (lasting until `end_us`) and the file index.
    pub fn finish(mut self, end_us: i64) -> Result<(), WriteError> {
        if let Some(last) = self.pending.take() {
            let duration = end_us - last.time_us;
            self.write_packet(last, duration)?;
        }
        self.output.write_trailer()?;
        Ok(())
    }

    fn write_packet(&mut self, frame: Pending, duration_us: i64) -> Result<(), WriteError> {
        let mut ts = frame.time_us.rescale(MICROS, self.time_base);
        if let Some(last) = self.last_ts {
            if ts <= last {
                ts = last + 1; // timestamps must strictly increase
            }
        }
        self.last_ts = Some(ts);
        let duration = duration_us.rescale(MICROS, self.time_base).max(1);

        let mut packet = ffmpeg::Packet::copy(&frame.data);
        packet.set_stream(0);
        packet.set_pts(Some(ts));
        packet.set_dts(Some(ts));
        packet.set_duration(duration);
        if frame.key_frame {
            packet.set_flags(codec::packet::Flags::KEY);
        }
        packet.write(&mut self.output)?;
        Ok(())
    }
}

/// What the recorder needs to know when it starts.
pub struct RecorderSetup {
    pub path: PathBuf,
    /// Current video size.
    pub width: u32,
    pub height: u32,
    /// Last codec configuration packet received, if any.
    pub config: Option<Vec<u8>>,
    pub events: Sender<Event>,
    pub notify: Notifier,
}

enum Input {
    Packet(MediaPacket, Instant),
    Resized { width: u32, height: u32 },
    Mark(Instant),
    Stop(RecordingEnd, Instant),
}

/// Handle on a running recording (its own thread).
pub struct Recorder {
    input: Sender<Input>,
    thread: Option<JoinHandle<()>>,
}

impl Recorder {
    /// Checks that the file can be created and starts the recording thread.
    pub fn start(setup: RecorderSetup) -> Result<Self, RecordingProblem> {
        let folder = setup.path.parent().unwrap_or(Path::new("."));
        if let Err(err) = fs::create_dir_all(folder).and_then(|()| fs::File::create(&setup.path)) {
            tracing::error!(%err, path = %setup.path.display(), "cannot create recording file");
            return Err(RecordingProblem::CannotCreateFile);
        }
        let (tx, rx) = unbounded();
        let task = Task::new(setup);
        let thread = thread::Builder::new()
            .name("recorder".into())
            .spawn(move || task.run(&rx))
            .map_err(|err| {
                tracing::error!(%err, "cannot start recorder thread");
                RecordingProblem::WriteFailed
            })?;
        Ok(Self {
            input: tx,
            thread: Some(thread),
        })
    }

    /// Hands a packet to the recorder (non-blocking).
    pub fn push(&self, packet: &MediaPacket) {
        let _ = self
            .input
            .send(Input::Packet(packet.clone(), Instant::now()));
    }

    /// The video size changed (rotation).
    pub fn resized(&self, width: u32, height: u32) {
        let _ = self.input.send(Input::Resized { width, height });
    }

    /// Puts a cut marker at the current position.
    pub fn mark(&self) {
        let _ = self.input.send(Input::Mark(Instant::now()));
    }

    /// Finalizes the file and waits for it to be complete.
    pub fn stop(mut self, end: RecordingEnd) {
        let _ = self.input.send(Input::Stop(end, Instant::now()));
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::error!("recorder thread panicked");
            }
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        // Dropped without stop(): still finalize the file.
        if self.thread.is_some() {
            let _ = self
                .input
                .send(Input::Stop(RecordingEnd::MirrorStopped, Instant::now()));
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

/// State of the recording thread.
struct Task {
    path: PathBuf,
    width: u32,
    height: u32,
    config: Option<Vec<u8>>,
    /// New SPS/PPS (after a rotation) to put in front of the next frame.
    inline_config: Option<Vec<u8>>,
    writer: Option<(MkvWriter, Timeline)>,
    /// Cut markers, saved next to the video after each new one.
    cuts: CutFile,
    failed: bool,
    events: Sender<Event>,
    notify: Notifier,
    last_progress: Instant,
}

impl Task {
    fn new(setup: RecorderSetup) -> Self {
        Self {
            path: setup.path,
            width: setup.width,
            height: setup.height,
            config: setup.config,
            inline_config: None,
            writer: None,
            cuts: CutFile::default(),
            failed: false,
            events: setup.events,
            notify: setup.notify,
            last_progress: Instant::now(),
        }
    }

    fn run(mut self, input: &Receiver<Input>) {
        tracing::info!(path = %self.path.display(), "recording: waiting for a key frame");
        self.send(RecordingState::WaitingKeyFrame {
            path: self.path.clone(),
        });
        loop {
            match input.recv_timeout(PROGRESS_INTERVAL) {
                Ok(Input::Packet(packet, arrival)) => self.on_packet(packet, arrival),
                Ok(Input::Resized { width, height }) => {
                    self.width = width;
                    self.height = height;
                }
                Ok(Input::Mark(at)) => self.mark(at),
                Ok(Input::Stop(end, at)) => return self.finish(end, at),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return self.finish(RecordingEnd::MirrorStopped, Instant::now())
                }
            }
            if self.last_progress.elapsed() >= PROGRESS_INTERVAL {
                self.report_progress();
            }
        }
    }

    fn on_packet(&mut self, packet: MediaPacket, arrival: Instant) {
        if self.failed {
            return;
        }
        if packet.config {
            if self.writer.is_some() {
                self.inline_config = Some(packet.data.clone());
            }
            self.config = Some(packet.data);
            return;
        }
        let Some(pts) = packet.pts_us else { return };

        if self.writer.is_none() {
            if !packet.key_frame {
                return;
            }
            let Some(config) = &self.config else { return };
            match MkvWriter::create(&self.path, self.width, self.height, config) {
                Ok(writer) => {
                    tracing::info!(
                        width = self.width,
                        height = self.height,
                        "recording started"
                    );
                    self.writer = Some((writer, Timeline::new(pts, arrival)));
                    self.inline_config = None;
                }
                Err(err) => {
                    tracing::error!(%err, "cannot open recording file");
                    self.fail(RecordingProblem::CannotCreateFile);
                    return;
                }
            }
        }

        let data = match self.inline_config.take() {
            Some(mut merged) => {
                merged.extend_from_slice(&packet.data);
                merged
            }
            None => packet.data,
        };
        if let Some((writer, timeline)) = &mut self.writer {
            let time_us = timeline.map(pts, arrival);
            if let Err(err) = writer.write(data, time_us, packet.key_frame) {
                tracing::error!(%err, "recording write failed");
                self.fail(RecordingProblem::WriteFailed);
                return;
            }
        }
        if self.last_progress.elapsed() >= PROGRESS_INTERVAL || self.writer_just_started() {
            self.report_progress();
        }
    }

    fn mark(&mut self, at: Instant) {
        let Some((_, timeline)) = &self.writer else {
            tracing::info!("cut ignored: recording has not started yet");
            return;
        };
        let time_ms = u64::try_from(timeline.time_at(at) / 1000).unwrap_or(0);
        self.cuts.markers.push(Marker { time_ms });
        if let Err(err) = self.cuts.save(&self.path) {
            tracing::error!(%err, "cannot save cut markers");
        }
        tracing::info!(time_ms, "cut marked");
        let event = Event::CutMarked {
            time: Duration::from_millis(time_ms),
            count: self.cuts.markers.len(),
        };
        if self.events.send(event).is_ok() {
            (self.notify)();
        }
    }

    fn writer_just_started(&self) -> bool {
        self.writer
            .as_ref()
            .is_some_and(|(_, timeline)| timeline.last_out_us == 0)
    }

    fn info(&self, at: Instant) -> RecordingInfo {
        let duration_us = self
            .writer
            .as_ref()
            .map_or(0, |(_, timeline)| timeline.time_at(at));
        RecordingInfo {
            path: self.path.clone(),
            bytes: fs::metadata(&self.path).map_or(0, |m| m.len()),
            duration: Duration::from_micros(u64::try_from(duration_us).unwrap_or(0)),
        }
    }

    fn report_progress(&mut self) {
        self.last_progress = Instant::now();
        if self.writer.is_some() && !self.failed {
            let info = self.info(Instant::now());
            self.send(RecordingState::Recording(info));
        }
    }

    fn fail(&mut self, problem: RecordingProblem) {
        self.failed = true;
        // Keep whatever was written: finalize best effort.
        if let Some((writer, timeline)) = self.writer.take() {
            let _ = writer.finish(timeline.time_at(Instant::now()));
        }
        self.send(RecordingState::Failed {
            path: Some(self.path.clone()),
            problem,
        });
    }

    fn finish(mut self, end: RecordingEnd, at: Instant) {
        if self.failed {
            return;
        }
        match self.writer.take() {
            None => {
                tracing::info!("recording stopped before the first key frame");
                let _ = fs::remove_file(&self.path);
                let _ = fs::remove_file(sidecar_path(&self.path));
                self.send(RecordingState::Failed {
                    path: None,
                    problem: RecordingProblem::NoVideo,
                });
            }
            Some((writer, timeline)) => {
                let end_us = timeline.time_at(at);
                match writer.finish(end_us) {
                    Ok(()) => {
                        // Rebuild the timeline-free info from the final values.
                        let info = RecordingInfo {
                            path: self.path.clone(),
                            bytes: fs::metadata(&self.path).map_or(0, |m| m.len()),
                            duration: Duration::from_micros(u64::try_from(end_us).unwrap_or(0)),
                        };
                        tracing::info!(?end, bytes = info.bytes, duration = ?info.duration, "recording finished");
                        self.send(RecordingState::Finished { info, end });
                    }
                    Err(err) => {
                        tracing::error!(%err, "could not finalize recording");
                        self.send(RecordingState::Failed {
                            path: Some(self.path.clone()),
                            problem: RecordingProblem::WriteFailed,
                        });
                    }
                }
            }
        }
    }

    fn send(&self, state: RecordingState) {
        if self.events.send(Event::Recording(state)).is_ok() {
            (self.notify)();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_is_readable_and_sortable() {
        let at = LocalDateTime {
            year: 2026,
            month: 3,
            day: 7,
            hour: 9,
            minute: 5,
            second: 2,
        };
        assert_eq!(recording_file_name(at), "PhoneCut 2026-03-07 09-05-02.mkv");
    }

    #[test]
    fn unique_path_adds_a_counter() {
        let dir = std::env::temp_dir().join(format!("phonecut-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let first = unique_path(&dir, "clip.mkv");
        assert_eq!(first, dir.join("clip.mkv"));
        fs::write(&first, b"x").unwrap();
        assert_eq!(unique_path(&dir, "clip.mkv"), dir.join("clip (2).mkv"));
        fs::write(dir.join("clip (2).mkv"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "clip.mkv"), dir.join("clip (3).mkv"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn timeline_follows_device_clock() {
        let t0 = Instant::now();
        let mut tl = Timeline::new(5_000_000, t0);
        assert_eq!(tl.map(5_000_000, t0), 0);
        // Arrival jitter does not matter while the device clock is consistent.
        assert_eq!(tl.map(5_016_666, t0 + Duration::from_millis(20)), 16_666);
        assert_eq!(tl.map(5_033_333, t0 + Duration::from_millis(30)), 33_333);
    }

    #[test]
    fn timeline_survives_device_clock_reset() {
        let t0 = Instant::now();
        let mut tl = Timeline::new(9_000_000, t0);
        tl.map(9_000_000, t0);
        tl.map(9_500_000, t0 + Duration::from_millis(500));
        // Encoder restarted (rotation): device PTS back near 0, 200 ms later.
        let out = tl.map(1_000, t0 + Duration::from_millis(700));
        assert_eq!(out, 700_000);
        // Then device deltas are used again.
        assert_eq!(tl.map(17_666, t0 + Duration::from_millis(717)), 716_666);
    }

    #[test]
    fn timeline_end_covers_static_screen() {
        let t0 = Instant::now();
        let mut tl = Timeline::new(0, t0);
        tl.map(0, t0);
        tl.map(1_000_000, t0 + Duration::from_secs(1));
        // No new frame for 4 s (static screen), then stop.
        assert_eq!(tl.time_at(t0 + Duration::from_secs(5)), 5_000_000);
    }
}
