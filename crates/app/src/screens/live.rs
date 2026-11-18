//! Live screen: shows the phone screen and drives the mirror lifecycle.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText, TextureHandle};
use phonecut_core::api::{DeviceInfo, MirrorEnd, MirrorState, VideoFrame};

use crate::texts;
use crate::widgets::video;

/// What the UI should ask the core to do.
#[derive(Debug, PartialEq, Eq)]
pub enum MirrorRequest {
    Start(String),
}

#[derive(Default)]
pub struct LiveScreen {
    state: Option<MirrorState>,
    texture: Option<TextureHandle>,
    /// Devices for which the mirror was already started automatically during
    /// the current connection (cleared when the device is unplugged), so a
    /// failing mirror is not restarted in a loop.
    auto_started: HashSet<String>,
    stats: DisplayStats,
}

/// How long it takes to hand frames to the GPU, logged periodically.
struct DisplayStats {
    since: Instant,
    frames: u32,
    upload: Duration,
}

impl Default for DisplayStats {
    fn default() -> Self {
        Self {
            since: Instant::now(),
            frames: 0,
            upload: Duration::ZERO,
        }
    }
}

impl DisplayStats {
    const INTERVAL: Duration = Duration::from_secs(10);

    fn record(&mut self, upload: Duration) {
        self.frames += 1;
        self.upload += upload;
        let elapsed = self.since.elapsed();
        if elapsed >= Self::INTERVAL {
            let fps = f64::from(self.frames) / elapsed.as_secs_f64();
            let avg_ms = self.upload.as_secs_f64() * 1000.0 / f64::from(self.frames);
            tracing::info!(
                fps = format!("{fps:.1}"),
                avg_upload_ms = format!("{avg_ms:.2}"),
                "display stats"
            );
            *self = Self::default();
        }
    }
}

impl LiveScreen {
    pub fn apply(&mut self, state: MirrorState) {
        if matches!(state, MirrorState::Stopped { .. }) {
            self.texture = None;
        }
        self.state = Some(state);
    }

    /// Decides whether the mirror must be started for the current device.
    pub fn plan(
        &mut self,
        devices: &[DeviceInfo],
        ready: Option<&DeviceInfo>,
    ) -> Option<MirrorRequest> {
        self.auto_started
            .retain(|serial| devices.iter().any(|d| &d.serial == serial));
        let device = ready?;
        // Switching phones: coming back to a previous one must start it again.
        self.auto_started.retain(|serial| serial == &device.serial);
        if self.auto_started.insert(device.serial.clone()) {
            Some(MirrorRequest::Start(device.serial.clone()))
        } else {
            None
        }
    }

    pub fn show_frame(&mut self, ctx: &egui::Context, frame: VideoFrame) {
        if !self.is_running() {
            return;
        }
        let started = Instant::now();
        video::upload(ctx, &mut self.texture, "phone-screen", &frame);
        self.stats.record(started.elapsed());
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, Some(MirrorState::Running { .. }))
    }

    /// True once there is an image to show.
    pub fn has_video(&self) -> bool {
        self.is_running() && self.texture.is_some()
    }

    /// Draws the phone screen, as large as possible with the right ratio.
    pub fn show_video(&self, ui: &mut egui::Ui) {
        if let Some(texture) = &self.texture {
            video::fit(ui, texture);
        }
    }

    /// Status below the connection message when no video is shown.
    /// Returns a request when the user asks to retry.
    pub fn show_status(
        &self,
        ui: &mut egui::Ui,
        ready: Option<&DeviceInfo>,
    ) -> Option<MirrorRequest> {
        let device = ready?;
        match &self.state {
            Some(MirrorState::Starting { .. }) | Some(MirrorState::Running { .. }) => {
                ui.add_space(24.0);
                ui.spinner();
                ui.label(RichText::new(texts::mirror_starting()).size(16.0));
                None
            }
            Some(MirrorState::Stopped { serial, end })
                if serial == &device.serial && *end != MirrorEnd::Requested =>
            {
                ui.add_space(24.0);
                ui.label(
                    RichText::new(texts::mirror_interrupted())
                        .size(18.0)
                        .strong(),
                );
                ui.add(
                    egui::Label::new(RichText::new(texts::mirror_interrupted_hint()).size(14.0))
                        .wrap(),
                );
                ui.add_space(8.0);
                let button = egui::Button::new(RichText::new(texts::mirror_retry()).size(18.0))
                    .min_size(egui::vec2(220.0, 44.0));
                ui.add(button)
                    .clicked()
                    .then(|| MirrorRequest::Start(device.serial.clone()))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonecut_core::api::DeviceState;

    fn dev(serial: &str) -> DeviceInfo {
        DeviceInfo {
            serial: serial.into(),
            state: DeviceState::Ready,
            manufacturer: None,
            model: None,
        }
    }

    #[test]
    fn starts_once_per_connection() {
        let mut live = LiveScreen::default();
        let a = dev("A");
        let devices = vec![a.clone()];
        assert_eq!(
            live.plan(&devices, Some(&a)),
            Some(MirrorRequest::Start("A".into()))
        );
        // Still connected (even if the mirror failed): no automatic restart.
        assert_eq!(live.plan(&devices, Some(&a)), None);
        // Unplugged then plugged again: start again.
        assert_eq!(live.plan(&[], None), None);
        assert_eq!(
            live.plan(&devices, Some(&a)),
            Some(MirrorRequest::Start("A".into()))
        );
    }

    #[test]
    fn switching_device_starts_the_new_one() {
        let mut live = LiveScreen::default();
        let (a, b) = (dev("A"), dev("B"));
        let devices = vec![a.clone(), b.clone()];
        assert!(live.plan(&devices, Some(&a)).is_some());
        assert_eq!(
            live.plan(&devices, Some(&b)),
            Some(MirrorRequest::Start("B".into()))
        );
        assert_eq!(
            live.plan(&devices, Some(&a)),
            Some(MirrorRequest::Start("A".into()))
        );
    }

    #[test]
    fn stop_clears_running_state() {
        let mut live = LiveScreen::default();
        live.apply(MirrorState::Running {
            serial: "A".into(),
            device_name: "Pixel".into(),
            width: 10,
            height: 20,
        });
        assert!(live.is_running());
        live.apply(MirrorState::Stopped {
            serial: "A".into(),
            end: MirrorEnd::Disconnected,
        });
        assert!(!live.is_running());
        assert!(!live.has_video());
    }
}
