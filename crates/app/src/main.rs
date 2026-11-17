// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod logging;
mod screens;
mod texts;

use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Color32, RichText};
use phonecut_core::api::{Command, Event, FrameSlot};
use phonecut_core::service::Core;
use phonecut_core::APP_NAME;

use screens::home::HomeScreen;
use screens::live::{LiveScreen, MirrorRequest};

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
    home: HomeScreen,
    live: LiveScreen,
}

impl PhoneCutApp {
    fn new(commands: Sender<Command>, events: Receiver<Event>, frames: Arc<FrameSlot>) -> Self {
        Self {
            commands,
            events,
            frames,
            home: HomeScreen::default(),
            live: LiveScreen::default(),
        }
    }

    fn send(&self, request: Option<MirrorRequest>) {
        let command = match request {
            Some(MirrorRequest::Start(serial)) => Command::StartMirror { serial },
            None => return,
        };
        if self.commands.send(command).is_err() {
            tracing::error!("core service is not running");
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
                other => self.home.apply(other),
            }
        }
        if let Some(frame) = self.frames.take() {
            let ctx = ui.ctx().clone();
            self.live.show_frame(&ctx, frame);
        }
        let request = self
            .live
            .plan(self.home.devices(), self.home.ready_device());
        self.send(request);

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
