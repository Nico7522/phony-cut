//! First-connection assistant: four illustrated steps, validated
//! automatically from what adb sees.

use std::path::Path;
use std::thread;

use crossbeam_channel::{unbounded, Receiver, TryRecvError};
use eframe::egui::{self, Color32, RichText, TextureHandle};
use phonecut_core::api::{DeviceInfo, DeviceState, VideoFrame};
use phonecut_core::{image, paths};

use crate::texts::{self, onboarding as t, Language};
use crate::widgets::video;

/// Steps of the assistant, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    DeveloperOptions,
    UsbDebugging,
    Plug,
    Authorize,
    Done,
}

impl Step {
    fn index(self) -> usize {
        self as usize
    }

    fn next(self) -> Self {
        match self {
            Self::DeveloperOptions => Self::UsbDebugging,
            Self::UsbDebugging => Self::Plug,
            Self::Plug => Self::Authorize,
            Self::Authorize | Self::Done => Self::Done,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::DeveloperOptions | Self::UsbDebugging => Self::DeveloperOptions,
            Self::Plug => Self::UsbDebugging,
            Self::Authorize => Self::Plug,
            Self::Done => Self::Authorize,
        }
    }

    /// Tutorial pictures (`assets/tutorial/<n>.jpg|png`) shown on this step.
    fn images(self) -> &'static [usize] {
        match self {
            Self::DeveloperOptions => &[1],
            Self::UsbDebugging => &[2, 3],
            Self::Plug => &[5],
            Self::Authorize => &[4],
            Self::Done => &[],
        }
    }
}

/// Step the assistant should show, given what adb sees. Steps 1–2 cannot be
/// detected (no USB debugging = the phone is invisible): the user moves
/// through them; as soon as the phone shows up, the assistant jumps ahead.
pub fn auto_step(current: Step, devices: &[DeviceInfo]) -> Step {
    let best = devices.iter().map(|d| &d.state).min_by_key(|s| match s {
        DeviceState::Ready => 0,
        DeviceState::Unauthorized | DeviceState::Authorizing => 1,
        _ => 2,
    });
    match best {
        Some(DeviceState::Ready) => Step::Done,
        Some(DeviceState::Unauthorized | DeviceState::Authorizing) => Step::Authorize,
        // Offline, no permissions…: plugged but not usable yet.
        Some(_) => Step::Plug,
        None if current >= Step::Authorize => Step::Plug, // unplugged
        None => current,
    }
}

/// Phone brands with known menu paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Brand {
    #[default]
    Xiaomi,
    Samsung,
    Pixel,
    OppoOnePlus,
    Other,
}

impl Brand {
    const ALL: [Brand; 5] = [
        Brand::Xiaomi,
        Brand::Samsung,
        Brand::Pixel,
        Brand::OppoOnePlus,
        Brand::Other,
    ];
}

pub enum OnboardingAction {
    /// The user finished or skipped the assistant.
    Close,
}

#[derive(Default)]
pub struct Onboarding {
    step: Option<Step>,
    brand: Brand,
    images: Vec<Option<TextureHandle>>,
    /// Language the pictures were loaded for.
    images_language: Option<Language>,
    loader: Option<Receiver<(usize, VideoFrame)>>,
}

impl Onboarding {
    pub fn is_open(&self) -> bool {
        self.step.is_some()
    }

    pub fn open(&mut self, ctx: &egui::Context) {
        self.step = Some(Step::DeveloperOptions);
        // Reload when the language changed (pictures may differ per language).
        if self.images_language != Some(texts::language()) {
            self.images.clear();
            self.loader = None;
        }
        if self.loader.is_none() && self.images.iter().all(Option::is_none) {
            self.images_language = Some(texts::language());
            self.loader = Some(load_images_in_background(ctx.clone()));
        }
    }

    pub fn close(&mut self) {
        self.step = None;
    }

    /// Follows the phone state (call every frame while open).
    pub fn update(&mut self, devices: &[DeviceInfo]) {
        if let Some(step) = self.step {
            self.step = Some(auto_step(step, devices));
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        device: Option<&DeviceInfo>,
    ) -> Option<OnboardingAction> {
        self.receive_images(ui.ctx());
        let step = self.step?;
        let mut action = None;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading(RichText::new(t::title()).size(26.0));
                ui.add_space(4.0);
                progress_dots(ui, step);
                ui.add_space(12.0);
                match step {
                    Step::Done => action = self.done(ui, device),
                    _ => self.step_page(ui, step, device),
                }
                ui.add_space(16.0);
                ui.separator();
                if let Some(next) = self.navigation(ui, step) {
                    if next == Step::Done && step != Step::Done {
                        // Skipping: the user can reopen it from the settings.
                        action = Some(OnboardingAction::Close);
                    } else {
                        self.step = Some(next);
                    }
                }
            });
        });
        action
    }

    fn step_page(&mut self, ui: &mut egui::Ui, step: Step, device: Option<&DeviceInfo>) {
        let (title, body) = t::step(step);
        ui.label(RichText::new(title).size(22.0).strong());
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new(body).size(16.0)).wrap());

        if matches!(step, Step::DeveloperOptions | Step::UsbDebugging) {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(t::brand_label());
                egui::ComboBox::from_id_salt("brand")
                    .selected_text(t::brand_name(self.brand))
                    .show_ui(ui, |ui| {
                        for brand in Brand::ALL {
                            ui.selectable_value(&mut self.brand, brand, t::brand_name(brand));
                        }
                    });
            });
            ui.add(
                egui::Label::new(
                    RichText::new(t::brand_path(self.brand, step == Step::DeveloperOptions))
                        .size(15.0)
                        .strong(),
                )
                .wrap(),
            );
        }

        if matches!(step, Step::Plug | Step::Authorize) {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spinner();
                let waiting = match (step, device) {
                    (Step::Plug, Some(d)) => texts::device(d).title,
                    (Step::Plug, None) => t::waiting_phone().to_owned(),
                    _ => t::waiting_authorization().to_owned(),
                };
                ui.label(RichText::new(waiting).size(16.0).italics());
            });
        }

        ui.add_space(10.0);
        ui.add(
            egui::Label::new(
                RichText::new(t::varies_notice()).color(Color32::from_rgb(160, 110, 0)),
            )
            .wrap(),
        );
        ui.add_space(8.0);
        self.pictures(ui, step);
        ui.add_space(8.0);
        egui::CollapsingHeader::new(RichText::new(t::help_title()).strong())
            .id_salt(("help", step.index()))
            .show(ui, |ui| {
                for tip in t::help(step) {
                    ui.add(egui::Label::new(format!("•  {tip}")).wrap());
                }
            });
    }

    fn pictures(&self, ui: &mut egui::Ui, step: Step) {
        let wanted = step.images();
        if wanted.is_empty() {
            return;
        }
        ui.horizontal_top(|ui| {
            for &n in wanted {
                match self.images.get(n).and_then(Option::as_ref) {
                    Some(texture) => {
                        let height = 360.0;
                        let [w, h] = texture.size().map(|v| v as f32);
                        let width = (w / h.max(1.0)) * height;
                        ui.allocate_ui(egui::vec2(width, height), |ui| video::fit(ui, texture));
                    }
                    None if self.loader.is_some() => {
                        ui.spinner();
                    }
                    None => {} // picture not provided: text only
                }
                ui.add_space(12.0);
            }
        });
    }

    fn done(&self, ui: &mut egui::Ui, device: Option<&DeviceInfo>) -> Option<OnboardingAction> {
        ui.label(RichText::new(t::done_title()).size(24.0).strong());
        if let Some(device) = device {
            ui.label(RichText::new(texts::device(device).title).size(18.0));
        }
        ui.add_space(8.0);
        ui.add(egui::Label::new(RichText::new(t::done_body()).size(16.0)).wrap());
        ui.add_space(12.0);
        let start = egui::Button::new(RichText::new(t::start()).size(20.0).color(Color32::WHITE))
            .fill(Color32::from_rgb(46, 125, 50))
            .min_size(egui::vec2(240.0, 52.0));
        ui.add(start).clicked().then_some(OnboardingAction::Close)
    }

    /// Previous / Next / Skip. Returns the step to go to.
    fn navigation(&self, ui: &mut egui::Ui, step: Step) -> Option<Step> {
        let mut target = None;
        ui.horizontal(|ui| {
            let big = |text: &str| {
                egui::Button::new(RichText::new(text).size(17.0)).min_size(egui::vec2(140.0, 40.0))
            };
            if step != Step::DeveloperOptions
                && step != Step::Done
                && ui.add(big(t::previous())).clicked()
            {
                target = Some(step.previous());
            }
            // Steps 1–2 cannot be detected: the user says when they are done.
            if matches!(step, Step::DeveloperOptions | Step::UsbDebugging)
                && ui.add(big(t::next())).clicked()
            {
                target = Some(step.next());
            }
            if step != Step::Done {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t::skip()).on_hover_text(t::skip_hint()).clicked() {
                        target = Some(Step::Done);
                    }
                });
            }
        });
        target
    }

    fn receive_images(&mut self, ctx: &egui::Context) {
        let Some(loader) = &self.loader else { return };
        let mut received = Vec::new();
        let finished = loop {
            match loader.try_recv() {
                Ok(image) => received.push(image),
                Err(TryRecvError::Empty) => break false,
                // The loading thread drops its sender when done.
                Err(TryRecvError::Disconnected) => break true,
            }
        };
        for (n, frame) in received {
            if self.images.len() <= n {
                self.images.resize(n + 1, None);
            }
            let mut texture = None;
            video::upload(ctx, &mut texture, &format!("tutorial-{n}"), &frame);
            self.images[n] = texture;
        }
        if finished {
            self.loader = None;
        }
    }
}

/// Decodes `assets/tutorial/1..=5.(png|jpg)` off the UI thread.
fn load_images_in_background(ctx: egui::Context) -> Receiver<(usize, VideoFrame)> {
    let (tx, rx) = unbounded();
    let language = texts::language();
    let spawned = thread::Builder::new()
        .name("tutorial-images".into())
        .spawn(move || {
            for n in 1..=5 {
                // Pictures in the interface language first (tutorial/en/…),
                // then the default ones (a French Xiaomi).
                let folders: &[&str] = match language {
                    Language::English => &["tutorial/en", "tutorial"],
                    Language::French => &["tutorial"],
                };
                let found = folders.iter().find_map(|folder| {
                    ["png", "jpg", "jpeg"].iter().find_map(|ext| {
                        paths::find_asset_file(&Path::new(folder).join(format!("{n}.{ext}")))
                    })
                });
                let Some(path) = found else { continue };
                match image::load_rgba(&path) {
                    Ok(frame) => {
                        let _ = tx.send((n, frame));
                        ctx.request_repaint();
                    }
                    Err(err) => tracing::warn!(%err, path = %path.display(), "tutorial image"),
                }
            }
            ctx.request_repaint();
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "cannot load tutorial images");
    }
    rx
}

fn progress_dots(ui: &mut egui::Ui, step: Step) {
    ui.horizontal(|ui| {
        for (i, label) in t::step_short().iter().enumerate() {
            let (color, strong) = match i.cmp(&step.index()) {
                std::cmp::Ordering::Less => (Color32::from_rgb(46, 160, 67), false),
                std::cmp::Ordering::Equal => (Color32::from_rgb(52, 101, 164), true),
                std::cmp::Ordering::Greater => (Color32::GRAY, false),
            };
            let text = RichText::new(format!("{}. {label}", i + 1)).color(color);
            ui.label(if strong { text.strong() } else { text });
            if i + 1 < t::step_short().len() {
                ui.label(RichText::new("—").color(Color32::GRAY));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(state: DeviceState) -> DeviceInfo {
        DeviceInfo {
            serial: "A".into(),
            state,
            manufacturer: None,
            model: None,
        }
    }

    #[test]
    fn manual_steps_stay_until_the_phone_appears() {
        assert_eq!(
            auto_step(Step::DeveloperOptions, &[]),
            Step::DeveloperOptions
        );
        assert_eq!(auto_step(Step::UsbDebugging, &[]), Step::UsbDebugging);
        assert_eq!(auto_step(Step::Plug, &[]), Step::Plug);
    }

    #[test]
    fn phone_detection_jumps_ahead() {
        let unauthorized = [device(DeviceState::Unauthorized)];
        assert_eq!(
            auto_step(Step::DeveloperOptions, &unauthorized),
            Step::Authorize
        );
        assert_eq!(
            auto_step(Step::Plug, &[device(DeviceState::Ready)]),
            Step::Done
        );
        assert_eq!(
            auto_step(Step::UsbDebugging, &[device(DeviceState::Offline)]),
            Step::Plug
        );
    }

    #[test]
    fn unplugging_goes_back_to_the_cable_step() {
        assert_eq!(auto_step(Step::Authorize, &[]), Step::Plug);
        assert_eq!(auto_step(Step::Done, &[]), Step::Plug);
    }

    #[test]
    fn best_device_wins() {
        let devices = [device(DeviceState::Offline), device(DeviceState::Ready)];
        assert_eq!(auto_step(Step::Plug, &devices), Step::Done);
    }

    #[test]
    fn navigation_order() {
        assert_eq!(Step::DeveloperOptions.next(), Step::UsbDebugging);
        assert_eq!(Step::Plug.previous(), Step::UsbDebugging);
        assert_eq!(Step::DeveloperOptions.previous(), Step::DeveloperOptions);
        assert_eq!(Step::Plug.images(), &[5]);
        assert_eq!(Step::Authorize.images(), &[4]);
    }
}
