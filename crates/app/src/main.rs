// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icon;
mod logging;
mod screens;
mod settings;
mod texts;
mod widgets;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Color32, RichText};
use phonycut_core::api::{
    Command, Event, ExportState, FrameSlot, PhoneButton, PhoneInput, RecordingState, SoundState,
};
use phonycut_core::platform;
use phonycut_core::service::Core;
use phonycut_core::APP_NAME;

use screens::clips::{ClipsAction, ClipsScreen};
use screens::home::HomeScreen;
use screens::live::{LiveScreen, MirrorRequest};
use screens::onboarding::{Onboarding, OnboardingAction};
use screens::record::{RecordAction, RecordPanel};
use screens::settings::SettingsAction;
use settings::Settings;

/// Upper bound on how long closing the window may wait for the core to clean up.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(8);

fn main() -> anyhow::Result<()> {
    // Before logging: the log file lives in the settings folder.
    let migration = phonycut_core::paths::migrate_legacy_app_data();
    let log_path = logging::init()?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log_file = %log_path.display(),
        "{APP_NAME} starting"
    );
    match migration {
        Ok(phonycut_core::paths::Migration::None) => {}
        Ok(done) => tracing::info!(?done, "settings taken over from PhoneCut"),
        Err(err) => tracing::warn!(%err, "could not take over the PhoneCut settings"),
    }

    // Settings first: the language is needed for every message.
    let settings = Settings::load();
    texts::set_language(settings.language.resolve());
    tracing::info!(
        output_dir = %settings.output_dir.display(),
        language = ?texts::language(),
        "settings loaded"
    );

    // Two PhonyCut at once would fight over the phone (two previews).
    let Some(_instance) = platform::single_instance() else {
        tracing::info!("{APP_NAME} is already running");
        rfd::MessageDialog::new()
            .set_title(APP_NAME)
            .set_description(texts::already_running())
            .set_level(rfd::MessageLevel::Info)
            .show();
        return Ok(());
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_icon(egui::IconData {
                rgba: icon::window_icon(),
                width: icon::WINDOW_ICON_SIZE,
                height: icon::WINDOW_ICON_SIZE,
            })
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
            let app = PhonyCutApp::new(&started, settings);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Live,
    Clips,
    Settings,
}

struct PhonyCutApp {
    commands: Sender<Command>,
    events: Receiver<Event>,
    frames: Arc<FrameSlot>,
    player_frames: Arc<FrameSlot>,
    settings: Settings,
    tab: Tab,
    home: HomeScreen,
    live: LiveScreen,
    record: RecordPanel,
    clips: ClipsScreen,
    onboarding: Onboarding,
    /// The first-launch check for the assistant was done.
    onboarding_checked: bool,
    /// Video settings changed during a recording: restart the mirror after it.
    restart_after_recording: bool,
    /// Answer of the open folder dialog (runs on its own thread).
    folder_picker: Option<Receiver<Option<PathBuf>>>,
}

impl PhonyCutApp {
    fn new(core: &Core, settings: Settings) -> Self {
        Self {
            commands: core.commands(),
            events: core.events(),
            frames: core.frames(),
            player_frames: core.player_frames(),
            settings,
            tab: Tab::Live,
            home: HomeScreen::default(),
            live: LiveScreen::default(),
            record: RecordPanel::default(),
            clips: ClipsScreen::default(),
            onboarding: Onboarding::default(),
            onboarding_checked: false,
            restart_after_recording: false,
            folder_picker: None,
        }
    }

    /// Restarts the mirror with the current video settings.
    fn restart_mirror(&mut self) {
        if let Some(device) = self.home.ready_device() {
            let serial = device.serial.clone();
            tracing::info!("video settings changed, restarting the preview");
            self.send(Some(MirrorRequest::Start(serial)));
        }
    }

    fn handle_settings(&mut self, actions: Vec<SettingsAction>, ctx: &egui::Context) {
        for action in actions {
            match action {
                SettingsAction::VideoChanged => {
                    if let Err(err) = self.settings.save() {
                        tracing::warn!(%err, "cannot save settings");
                    }
                    if self.record.is_recording() {
                        self.restart_after_recording = true;
                    } else {
                        self.restart_mirror();
                    }
                }
                SettingsAction::PickFolder => self.pick_folder(ctx),
                SettingsAction::OpenFolder(folder) => {
                    self.handle_record(Some(RecordAction::OpenFolder(folder)), ctx);
                }
                SettingsAction::OpenAssistant => self.onboarding.open(ctx),
                SettingsAction::LanguageChanged => {
                    texts::set_language(self.settings.language.resolve());
                    tracing::info!(language = ?texts::language(), "language changed");
                    if let Err(err) = self.settings.save() {
                        tracing::warn!(%err, "cannot save settings");
                    }
                }
                SettingsAction::OpenLicenses => {
                    // Shipped as vendor/licenses next to the executable.
                    let marker =
                        std::path::Path::new("licenses").join("scrcpy-LICENSE-Apache-2.0.txt");
                    let folder =
                        phonycut_core::paths::find_vendor_file(&marker, "PHONYCUT_LICENSES")
                            .ok()
                            .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
                    match folder {
                        Some(folder) => {
                            if let Err(err) = platform::open_in_file_manager(&folder) {
                                tracing::warn!(%err, "cannot open licenses folder");
                            }
                        }
                        None => tracing::warn!("licenses folder not found"),
                    }
                }
            }
        }
    }

    fn close_onboarding(&mut self) {
        self.onboarding.close();
        if !self.settings.onboarding_done {
            self.settings.onboarding_done = true;
            if let Err(err) = self.settings.save() {
                tracing::warn!(%err, "cannot save settings");
            }
        }
    }

    fn switch_tab(&mut self, tab: Tab) {
        if tab == self.tab {
            return;
        }
        if self.tab == Tab::Clips {
            self.command(Command::Pause);
            let pending = self.clips.flush();
            self.handle_clips(pending);
        }
        self.tab = tab;
        if tab == Tab::Clips && self.clips.needs_listing() {
            self.handle_clips(vec![ClipsAction::Refresh]);
        }
    }

    /// Opens a recording in "Mes clips".
    fn edit_video(&mut self, path: PathBuf) {
        self.switch_tab(Tab::Clips);
        let actions = self.clips.open(path);
        self.handle_clips(actions);
    }

    fn handle_clips(&mut self, actions: Vec<ClipsAction>) {
        for action in actions {
            match action {
                ClipsAction::Refresh => self.command(Command::ListRecordings {
                    folder: self.settings.output_dir.clone(),
                }),
                ClipsAction::Open(path) => self.command(Command::OpenVideo { path }),
                ClipsAction::Play => self.command(Command::Play),
                ClipsAction::Pause => self.command(Command::Pause),
                ClipsAction::Seek(time) => self.command(Command::Seek { time }),
                ClipsAction::Save(video, cuts) => self.command(Command::SaveCuts { video, cuts }),
                ClipsAction::Export(request) => self.command(Command::Export(request)),
                ClipsAction::CancelExport => self.command(Command::CancelExport),
                ClipsAction::OpenFolder(folder) => {
                    if let Err(err) = platform::open_in_file_manager(&folder) {
                        tracing::warn!(%err, folder = %folder.display(), "cannot open folder");
                    }
                }
            }
        }
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let mut chosen = None;
        ui.horizontal(|ui| {
            let live = if self.record.is_recording() {
                format!("{}{}", texts::tab_live(), texts::tab_recording())
            } else {
                texts::tab_live().to_owned()
            };
            let tabs = [
                (Tab::Live, live),
                (Tab::Clips, texts::tab_clips().to_owned()),
                (Tab::Settings, texts::tab_settings().to_owned()),
            ];
            for (tab, label) in tabs {
                let text = RichText::new(label).size(17.0);
                if ui.selectable_label(self.tab == tab, text).clicked() {
                    chosen = Some(tab);
                }
            }
        });
        if let Some(tab) = chosen {
            self.switch_tab(tab);
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
                let mut dialog = rfd::FileDialog::new().set_title(texts::pick_folder_title());
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
            self.clips.invalidate();
            if let Err(err) = self.settings.save() {
                tracing::warn!(%err, "cannot save settings");
            }
        } else {
            tracing::warn!(folder = %folder.display(), "unusable output folder");
            self.record.set_notice(texts::folder_invalid());
        }
    }

    /// After a successful export: show the support link at most once a week
    /// (remembered in the settings).
    fn support_link_due(&mut self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if !settings::support_due(self.settings.support_shown_at, now) {
            return false;
        }
        self.settings.support_shown_at = Some(now);
        if let Err(err) = self.settings.save() {
            tracing::warn!(%err, "cannot save settings");
        }
        true
    }

    fn send(&self, request: Option<MirrorRequest>) {
        match request {
            Some(MirrorRequest::Start(serial)) => self.command(Command::StartMirror {
                serial,
                quality: self.settings.video_quality(),
                sound: self.settings.sound_options(),
            }),
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
            Some(RecordAction::MarkCut) => self.command(Command::MarkCut),
            Some(RecordAction::OpenFolder(folder)) => {
                // The default folder may not exist before the first recording.
                let opened = std::fs::create_dir_all(&folder)
                    .and_then(|()| platform::open_in_file_manager(&folder));
                if let Err(err) = opened {
                    tracing::warn!(%err, folder = %folder.display(), "cannot open folder");
                    self.record.set_notice(texts::open_folder_failed());
                }
            }
            Some(RecordAction::PickFolder) => self.pick_folder(ctx),
            Some(RecordAction::EditVideo(path)) => self.edit_video(path),
            Some(RecordAction::OpenClips) => self.switch_tab(Tab::Clips),
            None => {}
        }
    }

    /// Phone name, phone buttons, and how to control the phone.
    fn status_bar(&self, ui: &mut egui::Ui) -> Option<PhoneButton> {
        let mut pressed = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("●").color(Color32::from_rgb(46, 160, 67)));
            if let Some(device) = self.home.ready_device() {
                ui.label(RichText::new(texts::device(device).title).strong());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(texts::live_badge());
                let sound = texts::sound_status(self.live.sound(), self.settings.record_sound);
                if !sound.is_empty() {
                    ui.add_space(12.0);
                    let problem = matches!(
                        self.live.sound(),
                        Some(
                            SoundState::NoOutput
                                | SoundState::NeedsAndroid11
                                | SoundState::Unavailable
                        )
                    );
                    let text = RichText::new(sound);
                    ui.add(
                        egui::Label::new(if problem {
                            text.color(Color32::from_rgb(214, 120, 0))
                        } else {
                            text.weak()
                        })
                        .truncate(),
                    );
                }
            });
        });
        ui.horizontal(|ui| {
            let buttons = [
                (texts::phone_back(), PhoneButton::Back),
                (texts::phone_home(), PhoneButton::Home),
                (texts::phone_recents(), PhoneButton::AppSwitch),
                (texts::phone_volume_down(), PhoneButton::VolumeDown),
                (texts::phone_volume_up(), PhoneButton::VolumeUp),
            ];
            for (label, button) in buttons {
                if ui.button(label).clicked() {
                    pressed = Some(button);
                }
            }
            ui.add_space(8.0);
            let hint = if self.live.keyboard_active() {
                RichText::new(texts::keyboard_active()).color(Color32::from_rgb(77, 145, 230))
            } else {
                RichText::new(texts::control_hint()).weak()
            };
            ui.add(egui::Label::new(hint).truncate());
        });
        pressed
    }
}

impl eframe::App for PhonyCutApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let events: Vec<Event> = self.events.try_iter().collect();
        for event in events {
            match event {
                Event::Mirror(state) => self.live.apply(state),
                Event::Sound(state) => self.live.set_sound(state),
                Event::Recording(state) => {
                    if matches!(state, RecordingState::Finished { .. }) {
                        self.clips.invalidate(); // a new video exists
                    }
                    let ended = matches!(
                        state,
                        RecordingState::Finished { .. } | RecordingState::Failed { .. }
                    );
                    self.record.apply(state);
                    if ended && std::mem::take(&mut self.restart_after_recording) {
                        self.restart_mirror();
                    }
                }
                Event::CutMarked { time, count } => self.record.cut_marked(time, count),
                Event::Recordings(list) => self.clips.set_recordings(list),
                Event::Player(event) => self.clips.apply_player(event),
                Event::CutsSaveFailed { .. } => self.clips.cuts_save_failed(),
                Event::Export(state) => {
                    let support =
                        matches!(state, ExportState::Finished { .. }) && self.support_link_due();
                    self.clips.apply_export(state, support);
                }
                other => self.home.apply(other),
            }
        }
        self.poll_folder_picker();
        let ctx = ui.ctx().clone();
        // Keyboard shortcut: M marks a cut (unless typing in a text field).
        let typing = ctx.memory(|m| m.focused().is_some());
        if self.record.can_mark_cut() && !typing && ui.input(|i| i.key_pressed(egui::Key::M)) {
            self.command(Command::MarkCut);
        }
        if let Some(frame) = self.frames.take() {
            // Off screen: drop it (no texture upload).
            if self.tab == Tab::Live {
                self.live.show_frame(&ctx, frame);
            }
        }
        if let Some(frame) = self.player_frames.take() {
            self.clips.show_frame(&ctx, &frame);
        }
        let request = self
            .live
            .plan(self.home.devices(), self.home.ready_device());
        self.send(request);

        // First launch: the connection assistant.
        if !self.onboarding_checked {
            self.onboarding_checked = true;
            if !self.settings.onboarding_done {
                self.onboarding.open(&ctx);
            }
        }
        if self.onboarding.is_open() {
            self.onboarding.update(self.home.devices());
            let device = self.home.ready_device().or(self.home.devices().first());
            if let Some(OnboardingAction::Close) = self.onboarding.show(ui, device) {
                self.close_onboarding();
            }
            return;
        }

        egui::Panel::top("tabs").show(ui, |ui| self.tab_bar(ui));
        match self.tab {
            Tab::Clips => {
                let actions = self.clips.show(ui);
                self.handle_clips(actions);
                return;
            }
            Tab::Settings => {
                let picking = self.folder_picker.is_some();
                let actions = screens::settings::show(ui, &mut self.settings, picking);
                self.handle_settings(actions, &ctx);
                return;
            }
            Tab::Live => {}
        }

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
            let mut button = None;
            egui::Panel::top("status").show(ui, |ui| button = self.status_bar(ui));
            if let Some(button) = button {
                self.command(Command::PhoneInput(PhoneInput::Button(button)));
            }
            let mut inputs = Vec::new();
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(Color32::from_gray(24)))
                .show(ui, |ui| inputs = self.live.show_video(ui));
            for input in inputs {
                self.command(Command::PhoneInput(input));
            }
        } else {
            let mut retry = None;
            let mut help = false;
            egui::CentralPanel::default().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    self.home.show(ui);
                    retry = self.live.show_status(ui, self.home.ready_device());
                    if self.home.ready_device().is_none() {
                        ui.add_space(24.0);
                        let button =
                            egui::Button::new(RichText::new(texts::connection_help()).size(18.0))
                                .min_size(egui::vec2(240.0, 44.0));
                        help = ui.add(button).clicked();
                    }
                });
            });
            self.send(retry);
            if help {
                self.onboarding.open(&ctx);
            }
        }
    }
}

impl Drop for PhonyCutApp {
    fn drop(&mut self) {
        // Window closing: save pending clip edits (the core is still running
        // and handles commands in order, before its shutdown).
        let pending = self.clips.flush();
        self.handle_clips(pending);
    }
}
