//! Instant replay: the last seconds of the stream are always kept in memory,
//! as the packets the phone sent (no decoding), and can be saved to a `.mkv`
//! at any time, without re-encoding.
//!
//! The video is kept in whole groups of pictures (a key frame and what
//! follows), so a saved replay always starts on a key frame (one per second).
//! The sound shares the device clock: it is kept from the first kept picture
//! on. A capture reset (rotation) starts a new replay: the encoder restarts
//! with another size.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::record::{MkvWriter, WriteError};
use crate::scrcpy::protocol::MediaPacket;

/// Never keep more than this, whatever the duration and the bit rate.
const MAX_BYTES: usize = 512 * 1024 * 1024;

/// A key frame and the pictures until the next one.
#[derive(Debug, Default)]
struct Group {
    /// (device time µs, data, key frame)
    packets: Vec<(u64, Vec<u8>, bool)>,
    bytes: usize,
}

/// The last seconds of the stream.
#[derive(Debug)]
pub struct ReplayBuffer {
    duration_us: u64,
    size: (u32, u32),
    video_config: Option<Vec<u8>>,
    audio_config: Option<Vec<u8>>,
    groups: VecDeque<Group>,
    /// (device time µs, data)
    audio: VecDeque<(u64, Vec<u8>)>,
    bytes: usize,
    /// Device time of the newest picture and when it arrived, to make the
    /// saved file last until the save (static screen at the end).
    last: Option<(u64, Instant)>,
}

/// What a save needs, taken from the buffer (the buffer goes on filling).
#[derive(Debug)]
pub struct ReplaySnapshot {
    size: (u32, u32),
    video_config: Vec<u8>,
    audio_config: Option<Vec<u8>>,
    video: Vec<(u64, Vec<u8>, bool)>,
    audio: Vec<(u64, Vec<u8>)>,
    /// File time of the end (µs).
    end_us: i64,
}

impl ReplayBuffer {
    pub fn new(duration: Duration, width: u32, height: u32) -> Self {
        Self {
            duration_us: u64::try_from(duration.as_micros()).unwrap_or(u64::MAX),
            size: (width, height),
            video_config: None,
            audio_config: None,
            groups: VecDeque::new(),
            audio: VecDeque::new(),
            bytes: 0,
            last: None,
        }
    }

    /// New capture session (rotation…): a new replay starts.
    pub fn reset(&mut self, width: u32, height: u32) {
        self.size = (width, height);
        self.video_config = None;
        self.groups.clear();
        self.audio.clear();
        self.bytes = 0;
        self.last = None;
    }

    /// New length, without losing what is kept (a shorter one trims at the
    /// next picture).
    pub fn set_duration(&mut self, duration: Duration) {
        self.duration_us = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
    }

    pub fn set_video_config(&mut self, config: Vec<u8>) {
        self.video_config = Some(config);
    }

    pub fn set_audio_config(&mut self, config: Vec<u8>) {
        self.audio_config = Some(config);
    }

    pub fn push_video(&mut self, packet: &MediaPacket, arrival: Instant) {
        if packet.config {
            self.video_config = Some(packet.data.clone());
            return;
        }
        let Some(pts) = packet.pts_us else { return };
        if packet.key_frame {
            self.groups.push_back(Group::default());
        }
        // Pictures before the first key frame cannot be shown: dropped.
        let Some(group) = self.groups.back_mut() else {
            return;
        };
        group
            .packets
            .push((pts, packet.data.clone(), packet.key_frame));
        group.bytes += packet.data.len();
        self.bytes += packet.data.len();
        self.last = Some((pts, arrival));
        self.trim(pts);
    }

    pub fn push_audio(&mut self, packet: &MediaPacket) {
        if packet.config {
            self.audio_config = Some(packet.data.clone());
            return;
        }
        let Some(pts) = packet.pts_us else { return };
        if self.groups.is_empty() {
            return;
        }
        self.bytes += packet.data.len();
        self.audio.push_back((pts, packet.data.clone()));
    }

    /// Drops the oldest groups while the rest still covers the duration (or
    /// the memory is over the cap), and the sound before the first picture.
    fn trim(&mut self, newest: u64) {
        while self.groups.len() > 1 {
            let second_start = self.groups[1].packets.first().map_or(newest, |p| p.0);
            let covered_without_first = newest.saturating_sub(second_start);
            if covered_without_first < self.duration_us && self.bytes <= MAX_BYTES {
                break;
            }
            if let Some(group) = self.groups.pop_front() {
                self.bytes -= group.bytes;
            }
        }
        let start = self.start_us();
        while self
            .audio
            .front()
            .is_some_and(|(pts, _)| Some(*pts) < start)
        {
            if let Some((_, data)) = self.audio.pop_front() {
                self.bytes -= data.len();
            }
        }
    }

    /// Device time of the first kept picture.
    fn start_us(&self) -> Option<u64> {
        self.groups.front()?.packets.first().map(|p| p.0)
    }

    /// Seconds of video kept (0 when there is nothing to save yet).
    pub fn kept(&self) -> Duration {
        match (self.start_us(), self.last) {
            (Some(start), Some((last, _))) => Duration::from_micros(last.saturating_sub(start)),
            _ => Duration::ZERO,
        }
    }

    /// Copies what is kept, for a save at `now`. `None` when nothing can be
    /// saved yet (no key frame, no configuration).
    pub fn snapshot(&self, now: Instant) -> Option<ReplaySnapshot> {
        let start = self.start_us()?;
        let (last, arrival) = self.last?;
        let video_config = self.video_config.clone()?;
        let since_last =
            i64::try_from(now.saturating_duration_since(arrival).as_micros()).unwrap_or(0);
        let end_us = i64::try_from(last - start).unwrap_or(0) + since_last.max(0);
        Some(ReplaySnapshot {
            size: self.size,
            video_config,
            audio_config: self.audio_config.clone().filter(|_| !self.audio.is_empty()),
            video: self
                .groups
                .iter()
                .flat_map(|g| g.packets.iter().cloned())
                .map(|(pts, data, key)| (pts - start, data, key))
                .collect(),
            audio: self
                .audio
                .iter()
                .map(|(pts, data)| (pts - start, data.clone()))
                .collect(),
            end_us,
        })
    }
}

/// `PhonyCut Replay 2026-11-21 10-05-03.mkv`
pub fn replay_file_name(at: crate::platform::LocalDateTime) -> String {
    format!(
        "{} Replay {:04}-{:02}-{:02} {:02}-{:02}-{:02}.mkv",
        crate::APP_NAME,
        at.year,
        at.month,
        at.day,
        at.hour,
        at.minute,
        at.second
    )
}

impl ReplaySnapshot {
    /// Length of the saved replay.
    pub fn duration(&self) -> Duration {
        Duration::from_micros(u64::try_from(self.end_us).unwrap_or(0))
    }

    /// Writes the replay (blocking: call it off the stream thread).
    pub fn write(self, path: &Path) -> Result<PathBuf, WriteError> {
        let (width, height) = self.size;
        let mut writer = MkvWriter::create(
            path,
            width,
            height,
            &self.video_config,
            self.audio_config.as_deref(),
        )?;
        // Both tracks in time order, like a recording.
        let mut audio = self.audio.into_iter().peekable();
        for (time, data, key) in self.video {
            while let Some((audio_time, _)) = audio.peek() {
                if *audio_time > time {
                    break;
                }
                if let Some((audio_time, audio_data)) = audio.next() {
                    writer.write_audio(&audio_data, i64::try_from(audio_time).unwrap_or(0))?;
                }
            }
            writer.write(data, i64::try_from(time).unwrap_or(0), key)?;
        }
        for (audio_time, audio_data) in audio {
            writer.write_audio(&audio_data, i64::try_from(audio_time).unwrap_or(0))?;
        }
        writer.finish(self.end_us)?;
        Ok(path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(pts: u64, key: bool) -> MediaPacket {
        MediaPacket {
            pts_us: Some(pts),
            config: false,
            key_frame: key,
            data: vec![0; 1000],
        }
    }

    fn config() -> MediaPacket {
        MediaPacket {
            pts_us: None,
            config: true,
            key_frame: false,
            data: vec![1, 2, 3],
        }
    }

    /// 10 s at 30 fps, a key frame every second.
    fn filled(seconds: u64) -> (ReplayBuffer, Instant) {
        let mut buffer = ReplayBuffer::new(Duration::from_secs(seconds), 100, 200);
        let t0 = Instant::now();
        buffer.push_video(&config(), t0);
        for i in 0..300u64 {
            let pts = 1_000_000 + i * 33_333;
            buffer.push_video(
                &video(pts, i % 30 == 0),
                t0 + Duration::from_micros(i * 33_333),
            );
            buffer.push_audio(&MediaPacket {
                pts_us: Some(pts + 5_000),
                config: false,
                key_frame: false,
                data: vec![0; 300],
            });
        }
        (buffer, t0)
    }

    #[test]
    fn keeps_about_the_duration_from_a_key_frame() {
        let (buffer, _) = filled(3);
        let kept = buffer.kept();
        // Whole groups of 1 s: between 3 and 4 s.
        assert!(
            kept >= Duration::from_millis(2_900) && kept < Duration::from_secs(4),
            "{kept:?}"
        );
        let snapshot = buffer.snapshot(Instant::now()).unwrap();
        assert!(snapshot.video[0].2, "starts on a key frame");
        assert_eq!(snapshot.video[0].0, 0, "file time starts at 0");
        assert!(snapshot.audio.iter().all(|(t, _)| *t < 4_000_000));
    }

    #[test]
    fn nothing_to_save_before_a_key_frame() {
        let mut buffer = ReplayBuffer::new(Duration::from_secs(30), 100, 200);
        buffer.push_video(&config(), Instant::now());
        buffer.push_video(&video(1_000, false), Instant::now());
        assert!(buffer.snapshot(Instant::now()).is_none());
        assert_eq!(buffer.kept(), Duration::ZERO);
    }

    #[test]
    fn longer_duration_keeps_what_is_kept() {
        let (mut buffer, t0) = filled(3);
        let before = buffer.kept();
        buffer.set_duration(Duration::from_secs(60));
        // More pictures: nothing old is dropped any more.
        for i in 300..330u64 {
            let pts = 1_000_000 + i * 33_333;
            buffer.push_video(
                &video(pts, i % 30 == 0),
                t0 + Duration::from_micros(i * 33_333),
            );
        }
        assert!(buffer.kept() > before);
        // Shorter again: trimmed at the next picture.
        buffer.set_duration(Duration::from_secs(1));
        buffer.push_video(&video(1_000_000 + 330 * 33_333, false), t0);
        assert!(buffer.kept() < Duration::from_secs(2));
    }

    #[test]
    fn rotation_starts_a_new_replay() {
        let (mut buffer, _) = filled(30);
        assert!(buffer.kept() > Duration::from_secs(9));
        buffer.reset(200, 100);
        assert!(
            buffer.snapshot(Instant::now()).is_none(),
            "new config needed"
        );
        assert_eq!(buffer.size, (200, 100));
    }

    #[test]
    fn saved_file_lasts_until_the_save() {
        let (buffer, t0) = filled(3);
        // Saved 2 s after the last picture (static screen).
        let last_arrival = t0 + Duration::from_micros(299 * 33_333);
        let snapshot = buffer
            .snapshot(last_arrival + Duration::from_secs(2))
            .unwrap();
        let video_span = snapshot.video.last().unwrap().0;
        assert!(
            snapshot.duration() >= Duration::from_micros(video_span) + Duration::from_millis(1_990)
        );
    }
}
