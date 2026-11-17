//! Recording controls: record/stop button, live duration and size, result,
//! output folder.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText};
use phonecut_core::api::{RecordingEnd, RecordingInfo, RecordingState};

use crate::texts;

/// What the user asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum RecordAction {
    Start,
    Stop,
    OpenFolder(PathBuf),
    /// Open the system "choose a folder" dialog.
    PickFolder,
}

const RED: Color32 = Color32::from_rgb(207, 34, 46);

#[derive(Default)]
pub struct RecordPanel {
    state: Option<RecordingState>,
    /// Start was requested, the core has not answered yet.
    starting: bool,
    /// The folder dialog is open.
    picking_folder: bool,
    /// Message to show (folder could not be used…).
    notice: Option<&'static str>,
}

impl RecordPanel {
    pub fn apply(&mut self, state: RecordingState) {
        self.starting = false;
        self.state = Some(state);
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
                Some(RecordingState::Finished { .. } | RecordingState::Failed { .. })
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
        ui.horizontal(|ui| {
            if self.is_recording() {
                let stop = egui::Button::new(RichText::new(texts::RECORD_STOP).size(18.0))
                    .min_size(egui::vec2(260.0, 44.0));
                if ui.add(stop).clicked() {
                    action = Some(RecordAction::Stop);
                }
                ui.add_space(12.0);
                self.live_indicator(ui);
            } else {
                let record = egui::Button::new(
                    RichText::new(texts::RECORD_START)
                        .size(18.0)
                        .color(Color32::WHITE),
                )
                .fill(RED)
                .min_size(egui::vec2(220.0, 44.0));
                if ui.add_enabled(can_record, record).clicked() {
                    self.starting = true;
                    self.notice = None;
                    action = Some(RecordAction::Start);
                }
            }
        });
        if let Some(result) = self.result(ui) {
            action = Some(result);
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

    fn live_indicator(&self, ui: &mut egui::Ui) {
        match &self.state {
            Some(RecordingState::Recording(info)) => {
                ui.label(
                    RichText::new(texts::RECORD_LIVE)
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
                ui.label(texts::RECORD_WAITING);
            }
        }
    }

    /// Result of the last recording.
    fn result(&self, ui: &mut egui::Ui) -> Option<RecordAction> {
        match &self.state {
            Some(RecordingState::Finished { info, end }) => {
                if *end == RecordingEnd::MirrorStopped {
                    ui.label(texts::RECORD_SAVED_AFTER_UNPLUG);
                }
                saved_line(ui, info)
            }
            Some(RecordingState::Failed { problem, .. }) => {
                ui.colored_label(RED, texts::recording_problem(*problem));
                None
            }
            _ => None,
        }
    }

    fn folder_row(&mut self, ui: &mut egui::Ui, folder: &Path) -> Option<RecordAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.label(texts::FOLDER_LABEL);
            ui.monospace(folder.display().to_string());
            if ui.button(texts::OPEN_FOLDER).clicked() {
                action = Some(RecordAction::OpenFolder(folder.to_path_buf()));
            }
            let change = ui.add_enabled(
                !self.is_recording() && !self.picking_folder,
                egui::Button::new(texts::FOLDER_CHANGE),
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
        ui.label(RichText::new(texts::RECORD_SAVED).strong());
        let name = info
            .path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        ui.label(format!(
            "{name}  ({}, {})",
            texts::duration(info.duration),
            texts::file_size(info.bytes)
        ));
        if let Some(folder) = info.path.parent() {
            if ui.button(texts::OPEN_FOLDER).clicked() {
                action = Some(RecordAction::OpenFolder(folder.to_path_buf()));
            }
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonecut_core::api::RecordingProblem;
    use std::time::Duration;

    fn info() -> RecordingInfo {
        RecordingInfo {
            path: PathBuf::from("a.mkv"),
            bytes: 10,
            duration: Duration::from_secs(1),
        }
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
