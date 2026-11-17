//! Background core service: owns adb, watches devices and talks to the UI
//! through channels. Never runs on the UI thread.

use std::collections::HashMap;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};

use crate::adb::{Adb, AdbError, ProductInfo};
use crate::api::{
    AdbProblem, AdbStatus, Command, DeviceInfo, DeviceState, Event, FrameSlot, MirrorEnd,
    MirrorState, RecordingProblem, RecordingState,
};
use crate::platform;
use crate::record::{recording_file_name, unique_path};
use crate::scrcpy::server::ServerOptions;
use crate::scrcpy::session::{MirrorSession, SessionContext};

pub use crate::api::Notifier;

/// How often the device list is re-read.
pub const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Handle on the running core service.
pub struct Core {
    commands: Sender<Command>,
    events: Receiver<Event>,
    frames: Arc<FrameSlot>,
    thread: Option<JoinHandle<()>>,
}

impl Core {
    /// Starts the service on its own thread. `notify` is called after each
    /// event and each new video frame, so the UI can repaint.
    pub fn start(notify: Notifier) -> std::io::Result<Self> {
        let (cmd_tx, cmd_rx) = unbounded();
        let (ev_tx, ev_rx) = unbounded();
        let frames = Arc::new(FrameSlot::default());
        let worker = Worker::new(cmd_rx, ev_tx, notify, Arc::clone(&frames));
        let thread = thread::Builder::new()
            .name("core-service".into())
            .spawn(move || worker.run())?;
        Ok(Self {
            commands: cmd_tx,
            events: ev_rx,
            frames,
            thread: Some(thread),
        })
    }

    pub fn commands(&self) -> Sender<Command> {
        self.commands.clone()
    }

    pub fn events(&self) -> Receiver<Event> {
        self.events.clone()
    }

    /// Latest decoded frame of the mirror.
    pub fn frames(&self) -> Arc<FrameSlot> {
        Arc::clone(&self.frames)
    }

    /// Asks the service to stop and waits up to `timeout` for it to finish
    /// cleaning up (stopping the adb server it started).
    pub fn shutdown(mut self, timeout: Duration) {
        let _ = self.commands.send(Command::Shutdown);
        let Some(thread) = self.thread.take() else {
            return;
        };
        let deadline = Instant::now() + timeout;
        while !thread.is_finished() {
            if Instant::now() >= deadline {
                tracing::warn!("core service did not stop in time");
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        if thread.join().is_err() {
            tracing::error!("core service thread panicked");
        }
    }
}

struct Worker {
    commands: Receiver<Command>,
    events: Sender<Event>,
    notify: Notifier,
    adb: Option<Adb>,
    server_running: bool,
    /// We started the adb server, so we stop it on exit. A server started
    /// by another tool is left alone.
    owns_server: bool,
    conflict_reported: bool,
    status: Option<AdbStatus>,
    /// Last list sent to the UI (`None` until the first one).
    devices: Option<Vec<DeviceInfo>>,
    products: HashMap<String, ProductInfo>,
    frames: Arc<FrameSlot>,
    mirror: Option<MirrorSession>,
}

impl Worker {
    fn new(
        commands: Receiver<Command>,
        events: Sender<Event>,
        notify: Notifier,
        frames: Arc<FrameSlot>,
    ) -> Self {
        Self {
            commands,
            events,
            notify,
            frames,
            mirror: None,
            adb: None,
            server_running: false,
            owns_server: false,
            conflict_reported: false,
            status: None,
            devices: None,
            products: HashMap::new(),
        }
    }

    fn run(mut self) {
        tracing::info!("core service started");
        self.set_status(AdbStatus::Starting);
        match Adb::locate() {
            Ok(adb) => {
                tracing::info!(path = %adb.path().display(), "using bundled adb");
                self.adb = Some(adb);
            }
            Err(err) => {
                tracing::error!(%err, "adb not found");
                self.set_status(AdbStatus::Problem(AdbProblem::AdbMissing));
            }
        }

        let mut next_poll = Instant::now();
        loop {
            if Instant::now() >= next_poll {
                self.poll();
                next_poll = Instant::now() + POLL_INTERVAL;
            }
            self.reap_mirror();
            let wait = next_poll.saturating_duration_since(Instant::now());
            match self.commands.recv_timeout(wait) {
                Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Command::RefreshDevices) => next_poll = Instant::now(),
                Ok(Command::StartMirror { serial }) => self.start_mirror(serial),
                Ok(Command::StopMirror) => self.stop_mirror(),
                Ok(Command::StartRecording { folder }) => self.start_recording(&folder),
                Ok(Command::StopRecording) => {
                    if let Some(mirror) = &self.mirror {
                        mirror.stop_recording();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        }

        self.stop_mirror();
        self.cleanup();
        tracing::info!("core service stopped");
    }

    fn start_mirror(&mut self, serial: String) {
        self.stop_mirror();
        let Some(adb) = self.adb.clone() else {
            self.send(Event::Mirror(MirrorState::Stopped {
                serial,
                end: MirrorEnd::Failed,
            }));
            return;
        };
        let ctx = SessionContext {
            adb,
            serial: serial.clone(),
            options: ServerOptions::default(),
            frames: Arc::clone(&self.frames),
            events: self.events.clone(),
            notify: Arc::clone(&self.notify),
        };
        match MirrorSession::start(ctx) {
            Ok(session) => self.mirror = Some(session),
            Err(err) => {
                tracing::error!(%err, "could not start mirror thread");
                self.send(Event::Mirror(MirrorState::Stopped {
                    serial,
                    end: MirrorEnd::Failed,
                }));
            }
        }
    }

    fn start_recording(&mut self, folder: &std::path::Path) {
        let result = match &self.mirror {
            Some(mirror) if !mirror.is_finished() => {
                let path = unique_path(folder, &recording_file_name(platform::local_now()));
                tracing::info!(path = %path.display(), "start recording");
                mirror.start_recording(path)
            }
            _ => Err(RecordingProblem::NotMirroring),
        };
        if let Err(problem) = result {
            tracing::warn!(?problem, "recording not started");
            self.send(Event::Recording(RecordingState::Failed {
                path: None,
                problem,
            }));
        }
    }

    fn stop_mirror(&mut self) {
        if let Some(session) = self.mirror.take() {
            tracing::info!(serial = %session.serial(), "stopping mirror");
            session.stop();
        }
    }

    /// Joins a session that ended on its own (unplug, error).
    fn reap_mirror(&mut self) {
        if self.mirror.as_ref().is_some_and(MirrorSession::is_finished) {
            self.stop_mirror();
        }
    }

    fn poll(&mut self) {
        let Some(adb) = self.adb.clone() else {
            return;
        };

        if !self.server_running {
            match adb.start_server() {
                Ok(start) => {
                    tracing::info!(started_by_us = start.started_by_us, "adb server ready");
                    self.server_running = true;
                    self.owns_server |= start.started_by_us;
                    if start.version_conflict {
                        self.report_conflict();
                    }
                }
                Err(err) => {
                    self.on_adb_failure(&err);
                    return;
                }
            }
        }

        match adb.devices() {
            Ok((mut devices, output)) => {
                if output.reports_version_conflict() {
                    self.report_conflict();
                }
                self.set_status(AdbStatus::Ready);
                self.fill_product_info(&adb, &mut devices);
                self.set_devices(devices);
            }
            Err(err) => {
                self.server_running = false;
                self.on_adb_failure(&err);
            }
        }
    }

    fn fill_product_info(&mut self, adb: &Adb, devices: &mut [DeviceInfo]) {
        self.products
            .retain(|serial, _| devices.iter().any(|d| &d.serial == serial));
        for device in devices.iter_mut() {
            if device.state != DeviceState::Ready {
                continue;
            }
            if !self.products.contains_key(&device.serial) {
                match adb.product_info(&device.serial) {
                    Ok(info) => {
                        self.products.insert(device.serial.clone(), info);
                    }
                    Err(err) => {
                        tracing::debug!(%err, "could not read product info");
                        continue;
                    }
                }
            }
            if let Some(info) = self.products.get(&device.serial) {
                device.manufacturer.clone_from(&info.manufacturer);
                if info.model.is_some() {
                    device.model.clone_from(&info.model);
                }
            }
        }
    }

    fn on_adb_failure(&mut self, err: &AdbError) {
        if self.status != Some(AdbStatus::Problem(AdbProblem::ServerFailed)) {
            tracing::error!(%err, "adb server unavailable, will retry");
        }
        self.set_status(AdbStatus::Problem(AdbProblem::ServerFailed));
        self.set_devices(Vec::new());
    }

    fn report_conflict(&mut self) {
        if !self.conflict_reported {
            tracing::warn!("another adb version is running on this PC");
            self.conflict_reported = true;
            self.send(Event::AdbConflict);
        }
    }

    fn set_status(&mut self, status: AdbStatus) {
        if self.status.as_ref() != Some(&status) {
            tracing::info!(?status, "adb status changed");
            self.status = Some(status.clone());
            self.send(Event::AdbStatus(status));
        }
    }

    fn set_devices(&mut self, devices: Vec<DeviceInfo>) {
        if self.devices.as_ref() != Some(&devices) {
            for d in &devices {
                tracing::info!(serial = %d.serial, state = ?d.state, name = %d.display_name(), "device");
            }
            if devices.is_empty() {
                tracing::info!("no device connected");
            }
            self.devices = Some(devices.clone());
            self.send(Event::Devices(devices));
        }
    }

    fn send(&self, event: Event) {
        // The UI may already be gone during shutdown; nothing to do then.
        if self.events.send(event).is_ok() {
            (self.notify)();
        }
    }

    fn cleanup(&mut self) {
        if !self.owns_server {
            return;
        }
        if let Some(adb) = &self.adb {
            match adb.kill_server() {
                Ok(()) => tracing::info!("adb server stopped"),
                Err(err) => tracing::warn!(%err, "could not stop adb server"),
            }
        }
    }
}
