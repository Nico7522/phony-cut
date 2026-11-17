// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod logging;
mod screens;
mod settings;
mod texts;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Color32, RichText};
use phonecut_core::api::{Command, Event, FrameSlot};
use phonecut_core::platform;
use phonecut_core::service::Core;
use phonecut_core::APP_NAME;

use screens::home::HomeScreen;
use screens::live::{LiveScreen, MirrorRequest};
use screens::record::{RecordAction, RecordPanel};
use settings::Settings;

/// Upper bound on how long closing the window may wait for the core to clean up.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(8);

fn main() -> anyhow::Result<()> {
    let log_path = logging::init()?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log_file = %log_path.display(),
        "{APP_NAME} starting"
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([480.0, 360.0]),
        ..Default::default()
    };

    // The core is started once the window exists (it needs the egui context to
    // wake the UI) but is owned here so it can be shut down after the window closes.
    let mut core: Option<Core> = None;
    let result = eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| {
            let ctx = cc.egui_ctx.clone();
            let started = Core::start(Arc::new(move || ctx.request_repaint()))?;
            let app = PhoneCutApp::new(started.commands(), started.events(), started.frames());
            core = Some(started);
            Ok(Box::new(app))
        }),
    );

    if let Some(core) = core {
        core.shutdown(SHUTDOWN_TIMEOUT);
    }

    match result {
        Ok(()) => {
            tracing::info!("{APP_NAME} closed");
            Ok(())
        }
        Err(err) => {
            tracing::error!(%err, "window could not be created");
            Err(anyhow::anyhow!("window could not be created: {err}"))
        }
    }
}

struct PhoneCutApp {
    commands: Sender<Command>,
    events: Receiver<Event>,
    frames: Arc<FrameSlot>,
    settings: Settings,
    home: HomeScreen,
    live: LiveScreen,
    record: RecordPanel,
    /// Answer of the open folder dialog (runs on its own thread).
    folder_picker: Option<Receiver<Option<PathBuf>>>,
}

impl PhoneCutApp {
    fn new(commands: Sender<Command>, events: Receiver<Event>, frames: Arc<FrameSlot>) -> Self {
        let settings = Settings::load();
        tracing::info!(output_dir = %settings.output_dir.display(), "settings loaded");
        Self {
            commands,
            events,
            frames,
            settings,
            home: HomeScreen::default(),
            live: LiveScreen::default(),
            record: RecordPanel::default(),
            folder_picker: None,
        }
    }

    /// Opens the system folder dialog without blocking the UI thread.
    fn pick_folder(&mut self, ctx: &egui::Context) {
        if self.folder_picker.is_some() {
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        let start = self.settings.output_dir.clone();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("folder-dialog".into())
            .spawn(move || {
                let mut dialog = rfd::FileDialog::new().set_title(texts::PICK_FOLDER_TITLE);
                // The default folder may not exist yet: start from its parent.
                if let Some(existing) = start.ancestors().find(|p| p.is_dir()) {
                    dialog = dialog.set_directory(existing);
                }
                let _ = tx.send(dialog.pick_folder());
                ctx.request_repaint();
            });
        match spawned {
            Ok(_) => {
                self.folder_picker = Some(rx);
                self.record.set_picking_folder(true);
            }
            Err(err) => tracing::error!(%err, "cannot open folder dialog"),
        }
    }

    /// Applies the folder chosen in the dialog, once it is closed.
    fn poll_folder_picker(&mut self) {
        let Some(rx) = &self.folder_picker else {
            return;
        };
        let answer = match rx.try_recv() {
            Ok(answer) => answer,
            Err(crossbeam_channel::TryRecvError::Empty) => return,
            Err(crossbeam_channel::TryRecvError::Disconnected) => None,
        };
        self.folder_picker = None;
        self.record.set_picking_folder(false);
        if let Some(folder) = answer {
            self.set_output_dir(folder);
        }
    }

    fn set_output_dir(&mut self, folder: PathBuf) {
        if std::fs::create_dir_all(&folder).is_ok() {
            tracing::info!(folder = %folder.display(), "output folder changed");
            self.settings.output_dir = folder;
            if let Err(err) = self.settings.save() {
                tracing::warn!(%err, "cannot save settings");
            }
        } else {
            tracing::warn!(folder = %folder.display(), "unusable output folder");
            self.record.set_notice(texts::FOLDER_INVALID);
        }
    }

    fn send(&self, request: Option<MirrorRequest>) {
        match request {
            Some(MirrorRequest::Start(serial)) => self.command(Command::StartMirror { serial }),
            None => {}
        }
    }

    fn command(&self, command: Command) {
        if self.commands.send(command).is_err() {
            tracing::error!("core service is not running");
        }
    }

    fn handle_record(&mut self, action: Option<RecordAction>, ctx: &egui::Context) {
        match action {
            Some(RecordAction::Start) => self.command(Command::StartRecording {
                folder: self.settings.output_dir.clone(),
            }),
            Some(RecordAction::Stop) => self.command(Command::StopRecording),
            Some(RecordAction::OpenFolder(folder)) => {
                // The default folder may not exist before the first recording.
                let opened = std::fs::create_dir_all(&folder)
                    .and_then(|()| platform::open_in_file_manager(&folder));
                if let Err(err) = opened {
                    tracing::warn!(%err, folder = %folder.display(), "cannot open folder");
                    self.record.set_notice(texts::OPEN_FOLDER_FAILED);
                }
            }
            Some(RecordAction::PickFolder) => self.pick_folder(ctx),
            None => {}
        }
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("●").color(Color32::from_rgb(46, 160, 67)));
            if let Some(device) = self.home.ready_device() {
                ui.label(RichText::new(texts::device(device).title).strong());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(texts::LIVE_BADGE);
            });
        });
    }
}

impl eframe::App for PhoneCutApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        for event in self.events.try_iter() {
            match event {
                Event::Mirror(state) => self.live.apply(state),
                Event::Recording(state) => self.record.apply(state),
                other => self.home.apply(other),
            }
        }
        self.poll_folder_picker();
        if let Some(frame) = self.frames.take() {
            let ctx = ui.ctx().clone();
            self.live.show_frame(&ctx, frame);
        }
        let request = self
            .live
            .plan(self.home.devices(), self.home.ready_device());
        self.send(request);

        // Recording controls: while there is video, or to show the last result.
        if self.live.has_video() || self.record.has_message() || self.record.is_recording() {
            let can_record = self.live.has_video();
            let mut action = None;
            egui::Panel::bottom("record").show(ui, |ui| {
                action = self.record.show(ui, can_record, &self.settings.output_dir);
            });
            let ctx = ui.ctx().clone();
            self.handle_record(action, &ctx);
        }

        if self.live.has_video() {
            egui::Panel::top("status").show(ui, |ui| self.status_bar(ui));
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(Color32::from_gray(24)))
                .show(ui, |ui| self.live.show_video(ui));
        } else {
            let mut retry = None;
            egui::CentralPanel::default().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    self.home.show(ui);
                    retry = self.live.show_status(ui, self.home.ready_device());
                });
            });
            self.send(retry);
        }
    }
}
