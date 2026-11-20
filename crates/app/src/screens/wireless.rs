//! Window to pair a phone over Wi-Fi: a QR code to scan from the phone's
//! "Wireless debugging", or the 6-digit code typed by the user.

use eframe::egui::{self, Color32, RichText};
use phonycut_core::api::PairingState;

use crate::texts::wireless as t;

/// What the window asks the core to do.
#[derive(Debug, PartialEq, Eq)]
pub enum WirelessAction {
    /// Show a (new) QR code and wait for the phone.
    Start,
    PairWithCode(String),
    Cancel,
}

#[derive(Default)]
pub struct WirelessDialog {
    open: bool,
    state: Option<PairingState>,
    /// Dark modules of the QR code, row by row, and its width.
    qr: Option<(Vec<bool>, usize)>,
    code: String,
}

impl WirelessDialog {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the window; the caller sends [`WirelessAction::Start`].
    pub fn open(&mut self) -> WirelessAction {
        self.open = true;
        self.state = None;
        self.qr = None;
        self.code.clear();
        WirelessAction::Start
    }

    pub fn is_paired(&self) -> bool {
        self.state == Some(PairingState::Paired)
    }

    /// Closes the window (e.g. the paired phone showed up).
    pub fn close(&mut self) -> WirelessAction {
        self.open = false;
        WirelessAction::Cancel
    }

    pub fn apply(&mut self, state: PairingState) {
        if let PairingState::WaitingForPhone { qr_payload } = &state {
            self.qr = qr_modules(qr_payload);
        }
        self.state = Some(state);
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Vec<WirelessAction> {
        let mut actions = Vec::new();
        if !self.open {
            return actions;
        }
        let mut open = true;
        egui::Window::new(t::title())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.add(egui::Label::new(t::intro()).wrap());
                ui.add_space(8.0);
                for (i, step) in t::steps().iter().enumerate() {
                    ui.add(egui::Label::new(format!("{}. {step}", i + 1)).wrap());
                }
                ui.add_space(8.0);
                ui.vertical_centered(|ui| self.qr_code(ui));
                ui.add_space(8.0);
                ui.add(egui::Label::new(t::or_code()).wrap());
                ui.horizontal(|ui| {
                    let edit = egui::TextEdit::singleline(&mut self.code)
                        .desired_width(120.0)
                        .char_limit(6)
                        .hint_text("123456");
                    let response = ui.add(edit);
                    self.code.retain(|c| c.is_ascii_digit());
                    let ready = self.code.len() == 6 && self.state != Some(PairingState::Pairing);
                    let enter =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui
                        .add_enabled(ready, egui::Button::new(t::pair_button()))
                        .clicked()
                        || (ready && enter)
                    {
                        actions.push(WirelessAction::PairWithCode(self.code.clone()));
                        self.state = Some(PairingState::Pairing);
                    }
                });
                if let Some(state) = &self.state {
                    ui.add_space(8.0);
                    let color = match state {
                        PairingState::Paired => Color32::from_rgb(46, 160, 67),
                        PairingState::Failed(_) => Color32::from_rgb(214, 120, 0),
                        _ => ui.visuals().text_color(),
                    };
                    ui.horizontal(|ui| {
                        if matches!(
                            state,
                            PairingState::Pairing | PairingState::WaitingForPhone { .. }
                        ) {
                            ui.spinner();
                        }
                        ui.add(
                            egui::Label::new(RichText::new(t::state(state)).color(color)).wrap(),
                        );
                    });
                    if matches!(state, PairingState::Failed(_)) && ui.button(t::retry()).clicked() {
                        actions.push(WirelessAction::Start);
                        self.state = None;
                    }
                }
                ui.add_space(8.0);
                if ui.button(t::close()).clicked() {
                    actions.push(self.close());
                }
            });
        if !open {
            actions.push(self.close());
        }
        actions
    }

    /// Black modules on white, with the 4-module margin readers need.
    fn qr_code(&self, ui: &mut egui::Ui) {
        const SIDE: f32 = 220.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(SIDE, SIDE), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::WHITE);
        let Some((modules, width)) = &self.qr else {
            return;
        };
        let cell = SIDE / (*width + 8) as f32;
        for (i, dark) in modules.iter().enumerate() {
            if *dark {
                let (x, y) = ((i % width + 4) as f32, (i / width + 4) as f32);
                let min = rect.min + egui::vec2(x * cell, y * cell);
                painter.rect_filled(
                    egui::Rect::from_min_size(min, egui::vec2(cell, cell)),
                    0.0,
                    Color32::BLACK,
                );
            }
        }
    }
}

fn qr_modules(payload: &str) -> Option<(Vec<bool>, usize)> {
    match qrcode::QrCode::new(payload.as_bytes()) {
        Ok(code) => Some((
            code.to_colors()
                .iter()
                .map(|c| *c == qrcode::Color::Dark)
                .collect(),
            code.width(),
        )),
        Err(err) => {
            tracing::error!(%err, "cannot draw the pairing QR code");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_code_is_square() {
        let (modules, width) =
            qr_modules("WIFI:T:ADB;S:phonycut-abcdefgh;P:abcdefghijkm;;").unwrap();
        assert_eq!(modules.len(), width * width);
        assert!(modules.iter().any(|d| *d) && modules.iter().any(|d| !*d));
    }

    #[test]
    fn opening_asks_for_a_qr_code_and_resets() {
        let mut dialog = WirelessDialog::default();
        dialog.apply(PairingState::Paired);
        assert_eq!(dialog.open(), WirelessAction::Start);
        assert!(dialog.is_open() && !dialog.is_paired());
    }
}
