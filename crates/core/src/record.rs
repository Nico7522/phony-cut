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

/// Duration of one AAC frame from the phone: 1024 samples at 48 kHz.
pub const AAC_FRAME_US: i64 = 1024 * 1_000_000 / crate::audio::DEVICE_RATE as i64;

/// Sets the codec parameters of a new stream, with `config` as extradata.
///
/// # Safety
/// `stream` must have just been created by `add_stream` (valid `codecpar`).
unsafe fn set_codec_parameters(
    stream: &mut format::stream::StreamMut,
    video: Option<(u32, u32)>,
    config: &[u8],
) -> Result<(), WriteError> {
    // SAFETY (caller): `codecpar` is valid and owned by the stream. FFmpeg
    // requires extradata to be av_malloc'ed with zeroed padding, and frees it
    // with the stream.
    let par = unsafe { (*stream.as_mut_ptr()).codecpar };
    unsafe {
        match video {
            Some((width, height)) => {
                (*par).codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
                (*par).codec_id = ffmpeg::ffi::AVCodecID::AV_CODEC_ID_H264;
                (*par).width = i32::try_from(width).unwrap_or(i32::MAX);
                (*par).height = i32::try_from(height).unwrap_or(i32::MAX);
            }
            None => {
                (*par).codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_AUDIO;
                (*par).codec_id = ffmpeg::ffi::AVCodecID::AV_CODEC_ID_AAC;
                (*par).sample_rate = crate::audio::DEVICE_RATE as i32;
                (*par).frame_size = 1024;
                (*par).ch_layout =
                    ffmpeg::ChannelLayout::default(i32::from(crate::audio::DEVICE_CHANNELS)).into();
            }
        }
        let extradata = ffmpeg::ffi::av_mallocz(config.len() + FFMPEG_PADDING).cast::<u8>();
        if extradata.is_null() {
            return Err(WriteError::OutOfMemory);
        }
        ptr::copy_nonoverlapping(config.as_ptr(), extradata, config.len());
        (*par).extradata = extradata;
        (*par).extradata_size = i32::try_from(config.len()).unwrap_or(0);
    }
    Ok(())
}

/// Per-stream timestamp state.
struct Track {
    index: usize,
    time_base: Rational,
    last_ts: Option<i64>,
}

impl Track {
    /// Timestamp in the stream time base, strictly increasing.
    fn timestamp(&mut self, time_us: i64) -> i64 {
        let mut ts = time_us.rescale(MICROS, self.time_base);
        if let Some(last) = self.last_ts {
            if ts <= last {
                ts = last + 1;
            }
        }
        self.last_ts = Some(ts);
        ts
    }
}

/// Writes one H.264 video track, and optionally the phone's AAC sound, into
/// a Matroska file. Packets of both tracks go through FFmpeg's interleaving
/// queue, so they reach the file in time order.
pub struct MkvWriter {
    output: format::context::Output,
    video: Track,
    audio: Option<Track>,
    pending: Option<Pending>,
}

impl MkvWriter {
    /// Creates the file. `config` is the video configuration packet (SPS/PPS,
    /// Annex-B) as sent by the server; `audio_config` the AAC
    /// AudioSpecificConfig when the sound is recorded too.
    pub fn create(
        path: &Path,
        width: u32,
        height: u32,
        config: &[u8],
        audio_config: Option<&[u8]>,
    ) -> Result<Self, WriteError> {
        crate::decode::init_ffmpeg();
        let mut output = format::output_as(path, "matroska")?;
        {
            let mut stream = output.add_stream(Option::<ffmpeg::Codec>::None)?;
            stream.set_time_base((1, 1000));
            // SAFETY: the stream was just created.
            unsafe { set_codec_parameters(&mut stream, Some((width, height)), config)? };
        }
        if let Some(audio_config) = audio_config {
            let mut stream = output.add_stream(Option::<ffmpeg::Codec>::None)?;
            stream.set_time_base((1, 1000));
            // SAFETY: the stream was just created.
            unsafe { set_codec_parameters(&mut stream, None, audio_config)? };
        }
        let mut options = Dictionary::new();
        // Small clusters + flushing: a crash loses at most ~1 s of video.
        options.set("cluster_time_limit", "1000");
        options.set("flush_packets", "1");
        // Interleaving waits for the other track at most 1 s (default 10 s).
        options.set("max_interleave_delta", "1000000");
        output.write_header_with(options)?;
        let track = |index: usize| {
            output
                .stream(index)
                .map(|s| Track {
                    index,
                    time_base: s.time_base(),
                    last_ts: None,
                })
                .ok_or(ffmpeg::Error::StreamNotFound)
        };
        let video = track(0)?;
        let audio = audio_config.map(|_| track(1)).transpose()?;
        Ok(Self {
            output,
            video,
            audio,
            pending: None,
        })
    }

    /// The file has a sound track.
    pub fn has_audio(&self) -> bool {
        self.audio.is_some()
    }

    /// Writes one AAC frame at file time `time_us` (ignored without a sound
    /// track).
    pub fn write_audio(&mut self, data: &[u8], time_us: i64) -> Result<(), WriteError> {
        let Some(track) = &mut self.audio else {
            return Ok(());
        };
        let ts = track.timestamp(time_us);
        let mut packet = ffmpeg::Packet::copy(data);
        packet.set_stream(track.index);
        packet.set_pts(Some(ts));
        packet.set_dts(Some(ts));
        packet.set_duration(AAC_FRAME_US.rescale(MICROS, track.time_base).max(1));
        packet.set_flags(codec::packet::Flags::KEY);
        packet.write_interleaved(&mut self.output)?;
        Ok(())
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
        let ts = self.video.timestamp(frame.time_us);
        let duration = duration_us.rescale(MICROS, self.video.time_base).max(1);

        let mut packet = ffmpeg::Packet::copy(&frame.data);
        packet.set_stream(self.video.index);
        packet.set_pts(Some(ts));
        packet.set_dts(Some(ts));
        packet.set_duration(duration);
        if frame.key_frame {
            packet.set_flags(codec::packet::Flags::KEY);
        }
        packet.write_interleaved(&mut self.output)?;
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
    /// AAC configuration when the phone's sound is streamed: the file gets a
    /// sound track.
    pub audio_config: Option<Vec<u8>>,
    pub events: Sender<Event>,
    pub notify: Notifier,
}

enum Input {
    Packet(MediaPacket, Instant),
    Audio(MediaPacket, Instant),
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

    /// Hands an AAC frame to the recorder (non-blocking).
    pub fn push_audio(&self, packet: &MediaPacket) {
        let _ = self
            .input
            .send(Input::Audio(packet.clone(), Instant::now()));
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

/// File time of the sound: offset of its first frame from the first video
/// frame, then its own continuity (the video clock may restart on rotation,
/// the sound clock does not).
struct AudioTimeline {
    offset: i64,
    timeline: Timeline,
}

/// State of the recording thread.
struct Task {
    path: PathBuf,
    width: u32,
    height: u32,
    config: Option<Vec<u8>>,
    /// New SPS/PPS (after a rotation) to put in front of the next frame.
    inline_config: Option<Vec<u8>>,
    audio_config: Option<Vec<u8>>,
    writer: Option<(MkvWriter, Timeline)>,
    /// Device PTS of the first recorded video frame (file time 0).
    video_origin: Option<u64>,
    audio_timeline: Option<AudioTimeline>,
    /// The video size changed after writing started: players need the file
    /// re-encoded into one fixed frame (see `normalize`).
    rotated: bool,
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
            audio_config: setup.audio_config,
            writer: None,
            video_origin: None,
            audio_timeline: None,
            rotated: false,
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
                Ok(Input::Audio(packet, arrival)) => self.on_audio(&packet, arrival),
                Ok(Input::Resized { width, height }) => {
                    self.width = width;
                    self.height = height;
                    if self.writer.is_some() {
                        self.rotated = true;
                    }
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
            let audio = self.audio_config.as_deref();
            match MkvWriter::create(&self.path, self.width, self.height, config, audio) {
                Ok(writer) => {
                    tracing::info!(
                        width = self.width,
                        height = self.height,
                        sound = writer.has_audio(),
                        "recording started"
                    );
                    self.writer = Some((writer, Timeline::new(pts, arrival)));
                    self.video_origin = Some(pts);
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

    /// Writes an AAC frame once the video has started; earlier sound is not
    /// part of the recording.
    fn on_audio(&mut self, packet: &MediaPacket, arrival: Instant) {
        if self.failed || packet.config {
            return;
        }
        let (Some(pts), Some(origin), Some((writer, _))) =
            (packet.pts_us, self.video_origin, &mut self.writer)
        else {
            return;
        };
        if !writer.has_audio() {
            return;
        }
        let timeline = match &mut self.audio_timeline {
            Some(timeline) => timeline,
            None => {
                // Same device clock as the video: its offset from the first
                // video frame is its file time.
                let offset = i64::try_from(pts).unwrap_or(0) - i64::try_from(origin).unwrap_or(0);
                if offset < 0 {
                    return; // captured before the first recorded image
                }
                self.audio_timeline.insert(AudioTimeline {
                    offset,
                    timeline: Timeline::new(pts, arrival),
                })
            }
        };
        let time_us = timeline.offset + timeline.timeline.map(pts, arrival);
        if let Err(err) = writer.write_audio(&packet.data, time_us) {
            tracing::error!(%err, "recording sound write failed");
            self.fail(RecordingProblem::WriteFailed);
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
                        let duration = Duration::from_micros(u64::try_from(end_us).unwrap_or(0));
                        if self.rotated {
                            self.fix_rotation_then_finish(end, duration);
                        } else {
                            self.finished(end, duration);
                        }
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

    fn finished(&self, end: RecordingEnd, duration: Duration) {
        let info = RecordingInfo {
            path: self.path.clone(),
            bytes: fs::metadata(&self.path).map_or(0, |m| m.len()),
            duration,
        };
        tracing::info!(?end, bytes = info.bytes, duration = ?info.duration, "recording finished");
        self.send(RecordingState::Finished { info, end });
    }

    /// Re-encodes the file on its own thread (it can take a while), so `stop`
    /// returns at once; `Finished` is sent when done. If the fix fails, the
    /// original file is kept: PhoneCut still reads it.
    fn fix_rotation_then_finish(self, end: RecordingEnd, duration: Duration) {
        tracing::info!("the phone rotated during the recording: re-encoding into one frame");
        self.send(RecordingState::Finalizing {
            path: self.path.clone(),
            percent: 0,
        });
        let spawned = thread::Builder::new()
            .name("rotation-fix".into())
            .spawn(move || {
                let started = Instant::now();
                let mut last = 0u8;
                let result = crate::normalize::fix_rotation(&self.path, &mut |fraction| {
                    let percent = (fraction.clamp(0.0, 1.0) * 100.0) as u8;
                    if percent >= last.saturating_add(2) {
                        last = percent;
                        self.send(RecordingState::Finalizing {
                            path: self.path.clone(),
                            percent,
                        });
                    }
                });
                match result {
                    Ok(()) => tracing::info!(took = ?started.elapsed(), "rotation fixed"),
                    Err(err) => tracing::error!(%err, "rotation fix failed: original file kept"),
                }
                self.finished(end, duration);
            });
        if let Err(err) = spawned {
            tracing::error!(%err, "cannot start the rotation fix");
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
