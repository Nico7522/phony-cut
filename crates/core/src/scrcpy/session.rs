//! A running screen mirror: starts the server, reads and decodes the stream on
//! its own thread, publishes frames to a [`FrameSlot`].

use std::io::{BufReader, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::adb::Adb;
use crate::api::PhoneInput;
use crate::api::{
    Event, FrameSlot, MirrorEnd, MirrorState, Notifier, RecordingEnd, RecordingProblem,
};
use crate::decode::{DecodeError, H264Decoder};
use crate::record::{Recorder, RecorderSetup};
use crate::scrcpy::control::{self, ControlMessage};
use crate::scrcpy::protocol::{self, ProtocolError, StreamItem, VideoCodec};
use crate::scrcpy::server::{RunningServer, ServerError, ServerOptions};

/// After the stream ends, how many times / how often to check whether the
/// phone is still there (adb may take a moment to notice an unplug).
const PRESENCE_CHECKS: u32 = 3;
const PRESENCE_CHECK_DELAY: Duration = Duration::from_millis(400);

/// How often decoding statistics are logged.
const STATS_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
enum SessionError {
    #[error("server: {0}")]
    Server(#[from] ServerError),
    #[error("stream: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("decoder: {0}")]
    Decode(#[from] DecodeError),
    #[error("unsupported codec {0:?} (expected H.264)")]
    Codec(VideoCodec),
}

#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    /// Clone of the video socket, so `stop()` can unblock the reading thread.
    socket: Mutex<Option<TcpStream>>,
    /// What a recorder starting mid-stream needs to know.
    stream: Mutex<StreamInfo>,
    /// Active recording, fed by the session thread.
    recorder: Mutex<Option<Recorder>>,
    /// Queue of the control writer thread (mouse / keyboard).
    control: Mutex<Option<crossbeam_channel::Sender<ControlMessage>>>,
    /// Clone of the control socket, closed by `request_stop()`.
    control_socket: Mutex<Option<TcpStream>>,
}

#[derive(Debug, Clone, Default)]
struct StreamInfo {
    size: Option<(u32, u32)>,
    /// Last codec configuration packet (SPS/PPS).
    config: Option<Vec<u8>>,
}

/// Locks a mutex, ignoring poisoning (the data stays consistent: every
/// critical section here is a simple swap or clone).
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Everything the session thread needs.
pub struct SessionContext {
    pub adb: Adb,
    pub serial: String,
    pub options: ServerOptions,
    pub frames: Arc<FrameSlot>,
    pub events: Sender<Event>,
    pub notify: Notifier,
}

impl SessionContext {
    fn send(&self, state: MirrorState) {
        if self.events.send(Event::Mirror(state)).is_ok() {
            (self.notify)();
        }
    }
}

/// Handle on a running mirror session.
pub struct MirrorSession {
    serial: String,
    shared: Arc<Shared>,
    events: Sender<Event>,
    notify: Notifier,
    thread: Option<JoinHandle<()>>,
}

impl MirrorSession {
    pub fn start(ctx: SessionContext) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let serial = ctx.serial.clone();
        let events = ctx.events.clone();
        let notify = Arc::clone(&ctx.notify);
        let thread_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("mirror-session".into())
            .spawn(move || run(ctx, &thread_shared))?;
        Ok(Self {
            serial,
            shared,
            events,
            notify,
            thread: Some(thread),
        })
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    /// Starts recording the stream into `path`.
    pub fn start_recording(&self, path: PathBuf) -> Result<(), RecordingProblem> {
        let stream = lock(&self.shared.stream).clone();
        let Some((width, height)) = stream.size else {
            return Err(RecordingProblem::NotMirroring);
        };
        let mut slot = lock(&self.shared.recorder);
        if slot.is_some() {
            return Err(RecordingProblem::AlreadyRecording);
        }
        *slot = Some(Recorder::start(RecorderSetup {
            path,
            width,
            height,
            config: stream.config,
            events: self.events.clone(),
            notify: Arc::clone(&self.notify),
        })?);
        Ok(())
    }

    /// Puts a cut marker in the running recording, if any.
    /// Forwards mouse / keyboard input to the phone. Ignored until the
    /// control socket is open.
    pub fn send_input(&self, input: &PhoneInput) {
        if let Some(control) = lock(&self.shared.control).as_ref() {
            for message in control::translate(input) {
                let _ = control.send(message);
            }
        }
    }

    pub fn mark_cut(&self) -> bool {
        match lock(&self.shared.recorder).as_ref() {
            Some(recorder) => {
                recorder.mark();
                true
            }
            None => false,
        }
    }

    /// Stops the recording, if any, and waits until the file is finalized.
    pub fn stop_recording(&self) -> bool {
        let recorder = lock(&self.shared.recorder).take();
        match recorder {
            Some(recorder) => {
                recorder.stop(RecordingEnd::Requested);
                true
            }
            None => false,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Asks the session to stop (non-blocking).
    pub fn request_stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        let socket = lock(&self.shared.socket).take();
        if let Some(socket) = socket {
            let _ = socket.shutdown(Shutdown::Both);
        }
        let control = lock(&self.shared.control_socket).take();
        if let Some(socket) = control {
            let _ = socket.shutdown(Shutdown::Both);
        }
        lock(&self.shared.control).take();
    }

    /// Stops the session and waits for its thread.
    pub fn stop(mut self) {
        self.request_stop();
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::error!("mirror session thread panicked");
            }
        }
    }
}

fn run(ctx: SessionContext, shared: &Shared) {
    tracing::info!(serial = %ctx.serial, "mirror starting");
    ctx.send(MirrorState::Starting {
        serial: ctx.serial.clone(),
    });

    let result = stream(&ctx, shared);
    // The stream is over: finalize any recording so the file stays playable.
    let recorder = lock(&shared.recorder).take();
    if let Some(recorder) = recorder {
        recorder.stop(RecordingEnd::MirrorStopped);
    }
    let end = if shared.stop.load(Ordering::Relaxed) {
        MirrorEnd::Requested
    } else {
        match &result {
            Ok(()) => MirrorEnd::Failed,
            Err(err) => {
                tracing::warn!(%err, "mirror ended");
                if still_connected(&ctx.adb, &ctx.serial) {
                    MirrorEnd::Failed
                } else {
                    MirrorEnd::Disconnected
                }
            }
        }
    };
    tracing::info!(?end, "mirror stopped");
    ctx.send(MirrorState::Stopped {
        serial: ctx.serial.clone(),
        end,
    });
}

fn stream(ctx: &SessionContext, shared: &Shared) -> Result<(), SessionError> {
    let mut server = RunningServer::start(&ctx.adb, &ctx.serial, &ctx.options)?;
    let connection = server.connect(&shared.stop, ctx.options.control)?;
    if let Some(control) = connection.control {
        start_control(control, shared);
    }
    let socket = connection.video;
    socket.set_nodelay(true).map_err(ProtocolError::from)?;
    socket.set_read_timeout(None).map_err(ProtocolError::from)?;

    // Publish a clone so request_stop() can unblock reads, then re-check the
    // flag in case stop was requested just before.
    let clone = socket.try_clone().map_err(ProtocolError::from)?;
    *lock(&shared.socket) = Some(clone);
    if shared.stop.load(Ordering::Relaxed) {
        return Ok(());
    }

    let mut reader = BufReader::with_capacity(1 << 16, socket);
    let device_name = protocol::read_device_name(&mut reader)?;
    let codec = protocol::read_codec(&mut reader)?;
    if codec != VideoCodec::H264 {
        return Err(SessionError::Codec(codec));
    }
    let session = protocol::read_first_session(&mut reader)?;
    tracing::info!(%device_name, width = session.width, height = session.height, "video stream open");
    let running = |width, height| MirrorState::Running {
        serial: ctx.serial.clone(),
        device_name: device_name.clone(),
        width,
        height,
    };
    ctx.send(running(session.width, session.height));
    lock(&shared.stream).size = Some((session.width, session.height));

    let mut decoder = H264Decoder::new()?;
    let mut stats = Stats::new();
    loop {
        match protocol::read_item(&mut reader)? {
            StreamItem::Session(session) => {
                // Capture reset (rotation…): the encoder restarts with new
                // parameters, start from a fresh decoder.
                tracing::info!(
                    width = session.width,
                    height = session.height,
                    "video size changed"
                );
                decoder = H264Decoder::new()?;
                ctx.send(running(session.width, session.height));
                *lock(&shared.stream) = StreamInfo {
                    size: Some((session.width, session.height)),
                    config: None, // a new one follows
                };
                if let Some(recorder) = lock(&shared.recorder).as_ref() {
                    recorder.resized(session.width, session.height);
                }
            }
            StreamItem::Packet(packet) => {
                if packet.config {
                    lock(&shared.stream).config = Some(packet.data.clone());
                }
                if let Some(recorder) = lock(&shared.recorder).as_ref() {
                    recorder.push(&packet);
                }
                let started = Instant::now();
                match decoder.decode(&packet) {
                    Ok(Some(frame)) => {
                        let decode_time = started.elapsed();
                        let dropped = ctx.frames.put(frame);
                        (ctx.notify)();
                        stats.record(decode_time, dropped);
                    }
                    Ok(None) => {}
                    // A corrupt packet must not end the mirror; the next key
                    // frame repairs the picture.
                    Err(err) => stats.error(&err),
                }
            }
        }
    }
}

/// Starts the control threads: a writer sending queued messages to the phone
/// and a reader draining what the phone sends back (device messages: none are
/// used yet, but an unread socket could block the server).
fn start_control(socket: TcpStream, shared: &Shared) {
    let (tx, rx) = crossbeam_channel::unbounded::<ControlMessage>();
    let (Ok(mut writer), Ok(mut reader), Ok(closer)) =
        (socket.try_clone(), socket.try_clone(), socket.try_clone())
    else {
        tracing::warn!("cannot clone the control socket: phone control disabled");
        return;
    };
    *lock(&shared.control_socket) = Some(closer);
    *lock(&shared.control) = Some(tx);

    let spawned = thread::Builder::new()
        .name("control-writer".into())
        .spawn(move || {
            // Ends when the session drops the sender or the socket closes.
            for message in rx {
                if let Err(err) = writer.write_all(&message.serialize()) {
                    tracing::debug!(%err, "control socket closed");
                    break;
                }
            }
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "cannot start control writer");
    }
    let spawned = thread::Builder::new()
        .name("control-reader".into())
        .spawn(move || {
            let mut buf = [0u8; 4096];
            while matches!(reader.read(&mut buf), Ok(n) if n > 0) {}
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "cannot start control reader");
    }
    drop(socket);
    tracing::info!("phone control enabled");
}

/// Polls adb a few times: an unplugged phone can still be listed briefly.
fn still_connected(adb: &Adb, serial: &str) -> bool {
    for attempt in 0..PRESENCE_CHECKS {
        if attempt > 0 {
            thread::sleep(PRESENCE_CHECK_DELAY);
        }
        if !adb.is_ready(serial) {
            return false;
        }
    }
    true
}

/// Periodic decode statistics in the log (helps diagnose latency).
struct Stats {
    since: Instant,
    frames: u32,
    decode_time: Duration,
    errors: u32,
    /// Frames replaced before the UI displayed them (UI too slow).
    dropped: u32,
}

impl Stats {
    fn new() -> Self {
        Self {
            since: Instant::now(),
            frames: 0,
            decode_time: Duration::ZERO,
            errors: 0,
            dropped: 0,
        }
    }

    fn record(&mut self, decode: Duration, dropped: bool) {
        self.frames += 1;
        self.decode_time += decode;
        self.dropped += u32::from(dropped);
        self.maybe_log();
    }

    fn error(&mut self, err: &DecodeError) {
        if self.errors == 0 {
            tracing::warn!(%err, "decode error (further errors counted in stats)");
        }
        self.errors += 1;
        self.maybe_log();
    }

    fn maybe_log(&mut self) {
        let elapsed = self.since.elapsed();
        if elapsed < STATS_INTERVAL {
            return;
        }
        let fps = f64::from(self.frames) / elapsed.as_secs_f64();
        let avg_ms = if self.frames > 0 {
            self.decode_time.as_secs_f64() * 1000.0 / f64::from(self.frames)
        } else {
            0.0
        };
        tracing::info!(
            fps = format!("{fps:.1}"),
            avg_decode_ms = format!("{avg_ms:.2}"),
            errors = self.errors,
            dropped_before_display = self.dropped,
            "mirror stats"
        );
        *self = Self::new();
    }
}
