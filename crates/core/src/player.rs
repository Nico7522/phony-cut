//! Plays a recording in the app: decoding on its own thread, frames paced on
//! the wall clock and published to a [`FrameSlot`], frame-accurate seeking.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender, TryRecvError};
use ffmpeg::util::mathematics::Rescale;
use ffmpeg::{format, media, Rational};
use ffmpeg_next as ffmpeg;

use crate::api::{Event, FrameSlot, LoadedVideo, Notifier, PlayerEvent};
use crate::decode::{init_ffmpeg, DecodeError, RgbaConverter};
use crate::markers::CutFile;

const MICROS: Rational = Rational(1, 1_000_000);
/// Position updates while playing (the UI interpolates nothing, 10/s is smooth
/// enough for a time label and a playhead).
const POSITION_INTERVAL_US: i64 = 100_000;

/// Player commands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Control {
    Play,
    Pause,
    Seek(Duration),
    Close,
}

pub struct PlayerSetup {
    pub path: PathBuf,
    pub frames: Arc<FrameSlot>,
    pub events: Sender<Event>,
    pub notify: Notifier,
}

/// Handle on an open video.
pub struct Player {
    control: Sender<Control>,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    pub fn open(setup: PlayerSetup) -> std::io::Result<Self> {
        let (tx, rx) = unbounded();
        let thread = thread::Builder::new()
            .name("player".into())
            .spawn(move || run(setup, &rx))?;
        Ok(Self {
            control: tx,
            thread: Some(thread),
        })
    }

    pub fn send(&self, control: Control) {
        let _ = self.control.send(control);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.control.send(Control::Close);
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::error!("player thread panicked");
            }
        }
    }
}

fn run(setup: PlayerSetup, control: &Receiver<Control>) {
    let send = |event: PlayerEvent| {
        if setup.events.send(Event::Player(event)).is_ok() {
            (setup.notify)();
        }
    };
    let mut playback = match Playback::open(&setup.path) {
        Ok(playback) => playback,
        Err(err) => {
            tracing::error!(%err, path = %setup.path.display(), "cannot open video");
            send(PlayerEvent::Failed {
                path: setup.path.clone(),
            });
            return;
        }
    };
    let cuts = CutFile::load(&setup.path).unwrap_or_else(|err| {
        tracing::warn!(%err, "unreadable markers file, ignored");
        CutFile::default()
    });
    send(PlayerEvent::Loaded(LoadedVideo {
        path: setup.path.clone(),
        duration: Duration::from_micros(u64::try_from(playback.duration_us).unwrap_or(0)),
        width: playback.width,
        height: playback.height,
        cuts,
    }));

    let mut state = State {
        setup: &setup,
        playback: &mut playback,
        playing: false,
        clock: (Instant::now(), 0),
        position_us: 0,
        last_reported_us: i64::MIN,
        at_end: false,
        deferred: VecDeque::new(),
    };
    state.seek(0);
    if let Err(err) = state.run(control) {
        tracing::error!(%err, "playback failed");
        send(PlayerEvent::Failed {
            path: setup.path.clone(),
        });
    }
}

/// Demuxer + decoder of one video stream.
struct Playback {
    input: format::context::Input,
    stream_index: usize,
    time_base: Rational,
    decoder: ffmpeg::decoder::Video,
    converter: RgbaConverter,
    frame: ffmpeg::util::frame::Video,
    packet: ffmpeg::Packet,
    eof_sent: bool,
    duration_us: i64,
    width: u32,
    height: u32,
}

impl Playback {
    fn open(path: &std::path::Path) -> Result<Self, ffmpeg::Error> {
        init_ffmpeg();
        let input = format::input(path)?;
        let stream = input
            .streams()
            .best(media::Type::Video)
            .ok_or(ffmpeg::Error::StreamNotFound)?;
        let stream_index = stream.index();
        let time_base = stream.time_base();
        let decoder = ffmpeg::codec::Context::from_parameters(stream.parameters())?
            .decoder()
            .video()?;
        let (width, height) = (decoder.width(), decoder.height());
        let duration_us = input.duration().max(0);
        Ok(Self {
            input,
            stream_index,
            time_base,
            decoder,
            converter: RgbaConverter::default(),
            frame: ffmpeg::util::frame::Video::empty(),
            packet: ffmpeg::Packet::empty(),
            eof_sent: false,
            duration_us,
            width,
            height,
        })
    }

    /// Decodes the next frame into `self.frame`; returns its time (µs), or
    /// `None` at the end of the file.
    fn next_frame(&mut self) -> Result<Option<i64>, DecodeError> {
        loop {
            match self.decoder.receive_frame(&mut self.frame) {
                Ok(()) => {
                    let pts = self.frame.timestamp().or(self.frame.pts()).unwrap_or(0);
                    return Ok(Some(pts.rescale(self.time_base, MICROS)));
                }
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::util::error::EAGAIN => {}
                Err(ffmpeg::Error::Eof) => return Ok(None),
                Err(err) => return Err(err.into()),
            }
            if self.eof_sent {
                return Ok(None);
            }
            match self.packet.read(&mut self.input) {
                Ok(()) => {
                    if self.packet.stream() == self.stream_index {
                        if let Err(err) = self.decoder.send_packet(&self.packet) {
                            tracing::debug!(%err, "packet skipped");
                        }
                    }
                }
                Err(ffmpeg::Error::Eof) => {
                    self.decoder.send_eof()?;
                    self.eof_sent = true;
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    /// Repositions on the key frame at or before `time_us`.
    fn seek_key_frame(&mut self, time_us: i64) -> Result<(), ffmpeg::Error> {
        // Container-level seek in AV_TIME_BASE (µs) units. The range end is
        // FFmpeg's inclusive `max_ts`: land on a key frame at or before.
        self.input.seek(time_us, ..time_us)?;
        self.decoder.flush();
        self.eof_sent = false;
        Ok(())
    }
}

struct State<'a> {
    setup: &'a PlayerSetup,
    playback: &'a mut Playback,
    playing: bool,
    /// Wall instant ↔ media time (µs) at which playback (re)started.
    clock: (Instant, i64),
    position_us: i64,
    last_reported_us: i64,
    at_end: bool,
    /// Commands read while coalescing seeks, to handle next.
    deferred: VecDeque<Control>,
}

impl State<'_> {
    fn run(&mut self, control: &Receiver<Control>) -> Result<(), DecodeError> {
        loop {
            if !self.playing {
                let next = match self.deferred.pop_front() {
                    Some(c) => c,
                    None => match control.recv() {
                        Ok(c) => c,
                        Err(_) => return Ok(()),
                    },
                };
                if !self.handle(next, control) {
                    return Ok(());
                }
                continue;
            }

            let Some(time_us) = self.playback.next_frame()? else {
                self.playing = false;
                self.at_end = true;
                self.report(true);
                self.send(PlayerEvent::Ended);
                continue;
            };
            // Wait until the frame is due, staying responsive to commands.
            let due = self.clock.0
                + Duration::from_micros(u64::try_from(time_us - self.clock.1).unwrap_or(0));
            let wait = due.saturating_duration_since(Instant::now());
            let interrupted = match self.deferred.pop_front() {
                Some(c) => Some(c),
                None => match control.recv_timeout(wait) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return Ok(()),
                },
            };
            if let Some(c) = interrupted {
                if !self.handle(c, control) {
                    return Ok(());
                }
                // Seek/pause: this frame is stale. Play: still show it.
                if !self.playing || matches!(c, Control::Seek(_)) {
                    continue;
                }
            }
            self.show_current(time_us)?;
            self.report(false);
        }
    }

    /// Returns `false` when the player must close.
    fn handle(&mut self, control: Control, rx: &Receiver<Control>) -> bool {
        match control {
            Control::Close => return false,
            Control::Play => {
                if self.at_end {
                    self.seek(0);
                }
                self.playing = true;
                self.clock = (Instant::now(), self.position_us);
                self.report(true);
            }
            Control::Pause => {
                self.playing = false;
                self.report(true);
            }
            Control::Seek(target) => {
                // Dragging the timeline sends many seeks: only the last matters.
                let mut target = target;
                loop {
                    match rx.try_recv() {
                        Ok(Control::Seek(next)) => target = next,
                        Ok(other) => {
                            self.deferred.push_back(other);
                            break;
                        }
                        Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                    }
                }
                self.seek(i64::try_from(target.as_micros()).unwrap_or(i64::MAX));
            }
        }
        true
    }

    /// Shows the frame at `target_us` (or the last one before the end).
    fn seek(&mut self, target_us: i64) {
        let target_us = target_us.clamp(0, self.playback.duration_us.max(0));
        if let Err(err) = self.playback.seek_key_frame(target_us) {
            tracing::warn!(%err, target_us, "seek failed");
        }
        self.at_end = false;
        // Decode forward to the requested frame without converting the ones
        // before it (the key frame may be up to ~1 s earlier).
        let mut shown = None;
        loop {
            match self.playback.next_frame() {
                Ok(Some(time_us)) => {
                    shown = Some(time_us);
                    // Frame-accurate: first frame whose time covers the target
                    // (1 ms tolerance for rounding).
                    if time_us + 1_000 >= target_us {
                        break;
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    tracing::warn!(%err, "decode error while seeking");
                    break;
                }
            }
        }
        if let Some(time_us) = shown {
            if let Err(err) = self.show_current(time_us) {
                tracing::warn!(%err, "cannot show frame");
            }
        }
        self.position_us = shown.unwrap_or(target_us).max(0);
        self.clock = (Instant::now(), self.position_us);
        self.report(true);
    }

    fn show_current(&mut self, time_us: i64) -> Result<(), DecodeError> {
        let pts = u64::try_from(time_us).ok();
        let frame = self.playback.converter.convert(&self.playback.frame, pts)?;
        self.setup.frames.put(frame);
        (self.setup.notify)();
        self.position_us = time_us;
        Ok(())
    }

    fn report(&mut self, force: bool) {
        if force || (self.position_us - self.last_reported_us).abs() >= POSITION_INTERVAL_US {
            self.last_reported_us = self.position_us;
            self.send(PlayerEvent::Position {
                time: Duration::from_micros(u64::try_from(self.position_us).unwrap_or(0)),
                playing: self.playing,
            });
        }
    }

    fn send(&self, event: PlayerEvent) {
        if self.setup.events.send(Event::Player(event)).is_ok() {
            (self.setup.notify)();
        }
    }
}
