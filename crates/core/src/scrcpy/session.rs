//! A running screen mirror: starts the server, reads and decodes the stream on
//! its own thread, publishes frames to a [`FrameSlot`].

use std::io::BufReader;
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::adb::Adb;
use crate::api::{Event, FrameSlot, MirrorEnd, MirrorState};
use crate::decode::{DecodeError, H264Decoder};
use crate::scrcpy::protocol::{self, ProtocolError, StreamItem, VideoCodec};
use crate::scrcpy::server::{RunningServer, ServerError, ServerOptions};

/// Wakes the UI after an event or a new frame.
pub type Notifier = Arc<dyn Fn() + Send + Sync>;

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
    thread: Option<JoinHandle<()>>,
}

impl MirrorSession {
    pub fn start(ctx: SessionContext) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let serial = ctx.serial.clone();
        let thread_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("mirror-session".into())
            .spawn(move || run(ctx, &thread_shared))?;
        Ok(Self {
            serial,
            shared,
            thread: Some(thread),
        })
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Asks the session to stop (non-blocking).
    pub fn request_stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        let socket = self
            .shared
            .socket
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(socket) = socket {
            let _ = socket.shutdown(Shutdown::Both);
        }
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
    let socket = server.connect(&shared.stop)?;
    socket.set_nodelay(true).map_err(ProtocolError::from)?;
    socket.set_read_timeout(None).map_err(ProtocolError::from)?;

    // Publish a clone so request_stop() can unblock reads, then re-check the
    // flag in case stop was requested just before.
    let clone = socket.try_clone().map_err(ProtocolError::from)?;
    *shared.socket.lock().unwrap_or_else(PoisonError::into_inner) = Some(clone);
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
            }
            StreamItem::Packet(packet) => {
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
