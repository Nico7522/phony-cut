//! Settings screen: video quality, smoothness, output folder, assistant.

use std::path::PathBuf;

use eframe::egui::{self, RichText};

use crate::settings::{Fps, QualityPreset, Settings};
use crate::texts::{self, settings as t};

#[derive(Debug, PartialEq, Eq)]
pub enum SettingsAction {
    /// Quality or smoothness changed (already written into the settings).
    VideoChanged,
    PickFolder,
    OpenFolder(PathBuf),
    OpenAssistant,
}

/// Draws the settings; edits `settings` in place.
pub fn show(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    picking_folder: bool,
) -> Vec<SettingsAction> {
    let mut actions = Vec::new();
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading(RichText::new(t::TITLE).size(24.0));
            ui.add_space(12.0);

            section(ui, t::QUALITY);
            for preset in [
                QualityPreset::Light,
                QualityPreset::Standard,
                QualityPreset::High,
            ] {
                let (name, hint) = t::quality(preset);
                if choice(ui, &mut settings.quality, preset, name, hint) {
                    actions.push(SettingsAction::VideoChanged);
                }
            }
            ui.add_space(12.0);

            section(ui, t::FPS);
            for fps in [Fps::Thirty, Fps::Sixty] {
                let (name, hint) = t::fps(fps);
                if choice(ui, &mut settings.fps, fps, name, hint) {
                    actions.push(SettingsAction::VideoChanged);
                }
            }
            ui.label(RichText::new(t::APPLIES_NOW).small());
            ui.add_space(16.0);

            section(ui, t::FOLDER);
            ui.horizontal(|ui| {
                ui.monospace(settings.output_dir.display().to_string());
            });
            ui.horizontal(|ui| {
                if ui.button(texts::OPEN_FOLDER).clicked() {
                    actions.push(SettingsAction::OpenFolder(settings.output_dir.clone()));
                }
                if ui
                    .add_enabled(!picking_folder, egui::Button::new(texts::FOLDER_CHANGE))
                    .clicked()
                {
                    actions.push(SettingsAction::PickFolder);
                }
            });
            ui.add_space(16.0);

            section(ui, t::ASSISTANT);
            if ui
                .button(RichText::new(t::ASSISTANT_BUTTON).size(16.0))
                .clicked()
            {
                actions.push(SettingsAction::OpenAssistant);
            }
        });
    });
    actions
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).size(18.0).strong());
    ui.add_space(4.0);
}

/// A radio button with an explanation below. Returns `true` when changed.
fn choice<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    current: &mut T,
    value: T,
    name: &str,
    hint: &str,
) -> bool {
    let changed = ui
        .radio_value(current, value, RichText::new(name).size(16.0))
        .changed();
    ui.indent(name, |ui| {
        ui.label(RichText::new(hint).weak());
    });
    ui.add_space(2.0);
    changed
}
