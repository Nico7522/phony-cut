//! Recording controls: record/stop button, live duration and size, result,
//! output folder.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};
use phonycut_core::api::{RecordingEnd, RecordingInfo, RecordingState, ReplayResult};

use crate::texts;

/// What the user asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum RecordAction {
    Start,
    Stop,
    MarkCut,
    /// Save the last seconds (instant replay).
    SaveReplay,
    OpenFolder(PathBuf),
    /// Open the system "choose a folder" dialog.
    PickFolder,
    /// Open this recording in "Mes clips".
    EditVideo(PathBuf),
    /// Go to "Mes clips".
    OpenClips,
}

const RED: Color32 = Color32::from_rgb(207, 34, 46);

#[derive(Default)]
pub struct RecordPanel {
    state: Option<RecordingState>,
    /// Start was requested, the core has not answered yet.
    starting: bool,
    /// The folder dialog is open.
    picking_folder: bool,
    /// Cut markers in the current recording: count and last position.
    cuts: usize,
    last_cut: Option<Duration>,
    /// Message to show (folder could not be used…).
    notice: Option<&'static str>,
    /// When the recording started waiting for its first key frame.
    waiting_since: Option<Instant>,
    /// Shortcut to mark a cut, shown on the button (e.g. "Ctrl+M").
    shortcut: String,
    /// Shortcut of the instant replay; `None` when it is turned off.
    replay_shortcut: Option<String>,
    /// Last replay save.
    replay_result: Option<ReplayResult>,
}

/// After this, a recording still waiting for an image explains why (the
/// phone screen is probably off: a sleeping phone sends no images).
const WAITING_HINT_AFTER: Duration = Duration::from_secs(3);

impl RecordPanel {
    pub fn apply(&mut self, state: RecordingState) {
        self.starting = false;
        if matches!(state, RecordingState::WaitingKeyFrame { .. }) {
            self.cuts = 0;
            self.last_cut = None;
            self.waiting_since = Some(Instant::now());
        } else {
            self.waiting_since = None;
        }
        self.state = Some(state);
    }

    pub fn set_shortcut(&mut self, label: String) {
        self.shortcut = label;
    }

    pub fn set_replay_shortcut(&mut self, label: Option<String>) {
        self.replay_shortcut = label;
    }

    pub fn replay_saved(&mut self, result: ReplayResult) {
        self.replay_result = Some(result);
    }

    /// The last replay result, under the buttons.
    fn replay_line(&self, ui: &mut egui::Ui) -> Option<RecordAction> {
        let mut action = None;
        match &self.replay_result {
            Some(ReplayResult::Saved { path, duration }) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(texts::replay_saved(duration.as_secs())).strong());
                    ui.label(
                        path.file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    );
                    if ui.button(texts::open_folder()).clicked() {
                        if let Some(folder) = path.parent() {
                            action = Some(RecordAction::OpenFolder(folder.to_path_buf()));
                        }
                    }
                });
            }
            Some(ReplayResult::Failed(problem)) => {
                ui.colored_label(RED, texts::replay_problem(*problem));
            }
            None => {}
        }
        action
    }

    pub fn cut_marked(&mut self, time: Duration, count: usize) {
        self.cuts = count;
        self.last_cut = Some(time);
    }

    /// Writing has started: cuts can be marked.
    pub fn can_mark_cut(&self) -> bool {
        matches!(self.state, Some(RecordingState::Recording(_)))
    }

    pub fn is_recording(&self) -> bool {
        self.starting
            || matches!(
                self.state,
                Some(RecordingState::WaitingKeyFrame { .. } | RecordingState::Recording(_))
            )
    }

    /// Something worth showing even without video (result of the last
    /// recording, error…).
    pub fn has_message(&self) -> bool {
        self.notice.is_some()
            || matches!(
                self.state,
                Some(
                    RecordingState::Finished { .. }
                        | RecordingState::Failed { .. }
                        | RecordingState::Finalizing { .. }
                )
            )
    }

    pub fn set_notice(&mut self, notice: &'static str) {
        self.notice = Some(notice);
    }

    pub fn set_picking_folder(&mut self, picking: bool) {
        self.picking_folder = picking;
    }

    /// Draws the panel. `can_record`: the mirror is running.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        can_record: bool,
        folder: &Path,
    ) -> Option<RecordAction> {
        let mut action = None;
        ui.add_space(6.0);
        // The three big buttons: record/stop, mark a cut, my clips.
        let big = egui::vec2(250.0, 48.0);
        ui.horizontal(|ui| {
            if self.is_recording() {
                let stop =
                    egui::Button::new(RichText::new(texts::record_stop()).size(18.0)).min_size(big);
                if ui.add(stop).clicked() {
                    action = Some(RecordAction::Stop);
                }
            } else {
                let record = egui::Button::new(
                    RichText::new(texts::record_start())
                        .size(18.0)
                        .color(Color32::WHITE),
                )
                .fill(RED)
                .min_size(big);
                if ui.add_enabled(can_record, record).clicked() {
                    self.starting = true;
                    self.notice = None;
                    action = Some(RecordAction::Start);
                }
            }
            let cut = egui::Button::new(RichText::new(texts::mark_cut(&self.shortcut)).size(18.0))
                .min_size(big);
            if ui
                .add_enabled(self.can_mark_cut(), cut)
                .on_hover_text(texts::mark_cut_hint(&self.shortcut))
                .on_disabled_hover_text(texts::mark_cut_hint(&self.shortcut))
                .clicked()
            {
                action = Some(RecordAction::MarkCut);
            }
            if let Some(replay) = &self.replay_shortcut {
                let button =
                    egui::Button::new(RichText::new(texts::save_replay(replay)).size(18.0))
                        .min_size(big);
                if ui
                    .add_enabled(can_record, button)
                    .on_hover_text(texts::save_replay_hint())
                    .clicked()
                {
                    action = Some(RecordAction::SaveReplay);
                }
            }
            let clips =
                egui::Button::new(RichText::new(texts::open_clips()).size(18.0)).min_size(big);
            if ui.add(clips).clicked() {
                action = Some(RecordAction::OpenClips);
            }
        });
        if self.is_recording() {
            ui.horizontal(|ui| {
                self.live_indicator(ui);
                if let Some(last) = self.last_cut {
                    ui.add_space(12.0);
                    ui.label(texts::cuts_summary(self.cuts, last));
                }
            });
        }
        if let Some(result) = self.result(ui) {
            action = Some(result);
        }
        if let Some(replay) = self.replay_line(ui) {
            action = Some(replay);
        }
        if let Some(notice) = self.notice {
            ui.colored_label(RED, notice);
        }
        ui.add_space(4.0);
        if let Some(folder_action) = self.folder_row(ui, folder) {
            action = Some(folder_action);
        }
        ui.add_space(6.0);
        action
    }

    /// One-line version for the full-screen bar: record / stop, mark a cut,
    /// and the live duration.
    pub fn show_compact(&mut self, ui: &mut egui::Ui, can_record: bool) -> Option<RecordAction> {
        let mut action = None;
        if self.is_recording() {
            if ui.button(texts::record_stop()).clicked() {
                action = Some(RecordAction::Stop);
            }
        } else {
            let record =
                egui::Button::new(RichText::new(texts::record_start()).color(Color32::WHITE))
                    .fill(RED);
            if ui.add_enabled(can_record, record).clicked() {
                self.starting = true;
                self.notice = None;
                action = Some(RecordAction::Start);
            }
        }
        if ui
            .add_enabled(
                self.can_mark_cut(),
                egui::Button::new(texts::mark_cut(&self.shortcut)),
            )
            .clicked()
        {
            action = Some(RecordAction::MarkCut);
        }
        if let Some(replay) = &self.replay_shortcut {
            if ui
                .button(texts::save_replay(replay))
                .on_hover_text(texts::save_replay_hint())
                .clicked()
            {
                action = Some(RecordAction::SaveReplay);
            }
        }
        if let Some(ReplayResult::Saved { duration, .. }) = &self.replay_result {
            ui.label(RichText::new(texts::replay_saved(duration.as_secs())).weak());
        }
        if self.is_recording() {
            ui.add_space(8.0);
            self.live_indicator(ui);
            if let Some(last) = self.last_cut {
                ui.label(texts::cuts_summary(self.cuts, last));
            }
        }
        action
    }

    fn live_indicator(&self, ui: &mut egui::Ui) {
        match &self.state {
            Some(RecordingState::Recording(info)) => {
                ui.label(
                    RichText::new(texts::record_live())
                        .color(RED)
                        .strong()
                        .size(18.0),
                );
                ui.label(
                    RichText::new(format!(
                        "{}  ·  {}",
                        texts::duration(info.duration),
                        texts::file_size(info.bytes)
                    ))
                    .size(18.0)
                    .monospace(),
                );
            }
            _ => {
                ui.spinner();
                ui.label(texts::record_waiting());
                if self.waiting_too_long(Instant::now()) {
                    ui.label(RichText::new(texts::record_waiting_asleep()).color(RED));
                }
            }
        }
    }

    /// Still no image long after the start: the phone is probably asleep.
    fn waiting_too_long(&self, now: Instant) -> bool {
        self.waiting_since
            .is_some_and(|since| now.saturating_duration_since(since) >= WAITING_HINT_AFTER)
    }

    /// Result of the last recording.
    fn result(&self, ui: &mut egui::Ui) -> Option<RecordAction> {
        match &self.state {
            Some(RecordingState::Finished { info, end }) => {
                if *end == RecordingEnd::MirrorStopped {
                    ui.label(texts::record_saved_after_unplug());
                }
                saved_line(ui, info)
            }
            Some(RecordingState::Failed { problem, .. }) => {
                ui.colored_label(RED, texts::recording_problem(*problem));
                None
            }
            Some(RecordingState::Finalizing { percent, .. }) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(texts::record_finalizing(*percent));
                });
                None
            }
            _ => None,
        }
    }

    fn folder_row(&mut self, ui: &mut egui::Ui, folder: &Path) -> Option<RecordAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.label(texts::folder_label());
            ui.monospace(folder.display().to_string());
            if ui.button(texts::open_folder()).clicked() {
                action = Some(RecordAction::OpenFolder(folder.to_path_buf()));
            }
            let change = ui.add_enabled(
                !self.is_recording() && !self.picking_folder,
                egui::Button::new(texts::folder_change()),
            );
            if change.clicked() {
                self.notice = None;
                action = Some(RecordAction::PickFolder);
            }
        });
        action
    }
}

fn saved_line(ui: &mut egui::Ui, info: &RecordingInfo) -> Option<RecordAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(texts::record_saved()).strong());
        let name = info
            .path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        ui.label(format!(
            "{name}  ({}, {})",
            texts::duration(info.duration),
            texts::file_size(info.bytes)
        ));
        let edit = egui::Button::new(RichText::new(texts::edit_video()).strong());
        if ui.add(edit).clicked() {
            action = Some(RecordAction::EditVideo(info.path.clone()));
        }
        if let Some(folder) = info.path.parent() {
            if ui.button(texts::open_folder()).clicked() {
                action = Some(RecordAction::OpenFolder(folder.to_path_buf()));
            }
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonycut_core::api::RecordingProblem;

    fn info() -> RecordingInfo {
        RecordingInfo {
            path: PathBuf::from("a.mkv"),
            bytes: 10,
            duration: Duration::from_secs(1),
        }
    }

    #[test]
    fn sleeping_phone_hint_after_a_few_seconds() {
        let mut panel = RecordPanel::default();
        panel.apply(RecordingState::WaitingKeyFrame {
            path: PathBuf::from("a.mkv"),
        });
        let since = panel.waiting_since.expect("waiting");
        assert!(!panel.waiting_too_long(since + Duration::from_secs(1)));
        assert!(panel.waiting_too_long(since + WAITING_HINT_AFTER));
        // Images arrive: no more hint.
        panel.apply(RecordingState::Recording(info()));
        assert!(!panel.waiting_too_long(since + Duration::from_secs(60)));
    }

    #[test]
    fn recording_lifecycle() {
        assert!(!RecordPanel::default().is_recording());
        let mut panel = RecordPanel {
            starting: true,
            ..RecordPanel::default()
        };
        assert!(panel.is_recording(), "pressed, waiting for the core");
        panel.apply(RecordingState::WaitingKeyFrame {
            path: PathBuf::from("a.mkv"),
        });
        assert!(panel.is_recording());
        panel.apply(RecordingState::Recording(info()));
        assert!(panel.is_recording());
        panel.apply(RecordingState::Finished {
            info: info(),
            end: RecordingEnd::Requested,
        });
        assert!(!panel.is_recording());
        assert!(panel.has_message());
    }

    #[test]
    fn failure_to_start_is_not_recording() {
        let mut panel = RecordPanel {
            starting: true,
            ..RecordPanel::default()
        };
        panel.apply(RecordingState::Failed {
            path: None,
            problem: RecordingProblem::CannotCreateFile,
        });
        assert!(!panel.is_recording());
        assert!(panel.has_message());
    }
}
