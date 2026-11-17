//! Home screen: connection status of the phone.

use eframe::egui::{self, Color32, RichText};
use phonecut_core::api::{AdbProblem, AdbStatus, DeviceInfo, DeviceState, Event};

use crate::texts;

/// What the home screen should currently show.
#[derive(Debug, PartialEq, Eq)]
enum View<'a> {
    Starting,
    Problem(AdbProblem),
    NoDevice,
    ChooseDevice,
    Device(&'a DeviceInfo),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Severity {
    Ok,
    Waiting,
    Error,
}

pub struct HomeScreen {
    adb_status: AdbStatus,
    devices: Vec<DeviceInfo>,
    /// Serial of the device the user works with.
    selected: Option<String>,
    adb_conflict: bool,
}

impl Default for HomeScreen {
    fn default() -> Self {
        Self {
            adb_status: AdbStatus::Starting,
            devices: Vec::new(),
            selected: None,
            adb_conflict: false,
        }
    }
}

impl HomeScreen {
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::AdbStatus(status) => self.adb_status = status,
            Event::Devices(devices) => {
                self.selected = reconcile_selection(self.selected.take(), &devices);
                self.devices = devices;
            }
            Event::AdbConflict => self.adb_conflict = true,
            // Handled by the live and record screens.
            Event::Mirror(_)
            | Event::Recording(_)
            | Event::CutMarked { .. }
            | Event::Recordings(_)
            | Event::Player(_)
            | Event::CutsSaveFailed { .. }
            | Event::Export(_) => {}
        }
    }

    pub fn devices(&self) -> &[DeviceInfo] {
        &self.devices
    }

    /// The device the user works with, when it is connected and authorized.
    pub fn ready_device(&self) -> Option<&DeviceInfo> {
        match self.view() {
            View::Device(device) if device.state == DeviceState::Ready => Some(device),
            _ => None,
        }
    }

    fn view(&self) -> View<'_> {
        match self.adb_status {
            AdbStatus::Starting => View::Starting,
            AdbStatus::Problem(problem) => View::Problem(problem),
            AdbStatus::Ready => {
                let selected = self
                    .selected
                    .as_ref()
                    .and_then(|s| self.devices.iter().find(|d| &d.serial == s));
                match (selected, self.devices.as_slice()) {
                    (Some(device), _) => View::Device(device),
                    (None, []) => View::NoDevice,
                    (None, [only]) => View::Device(only),
                    (None, _) => View::ChooseDevice,
                }
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let (severity, message) = match self.view() {
            View::Starting => (
                Severity::Waiting,
                texts::Message {
                    title: texts::STARTING.into(),
                    hint: texts::STARTING_HINT,
                },
            ),
            View::Problem(problem) => (Severity::Error, texts::adb_problem(problem)),
            View::NoDevice => (
                Severity::Waiting,
                texts::Message {
                    title: texts::NO_DEVICE.into(),
                    hint: texts::NO_DEVICE_HINT,
                },
            ),
            View::ChooseDevice => (
                Severity::Waiting,
                texts::Message {
                    title: texts::CHOOSE_DEVICE.into(),
                    hint: texts::CHOOSE_DEVICE_HINT,
                },
            ),
            View::Device(device) => (severity_of(&device.state), texts::device(device)),
        };

        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.25);
            ui.label(
                RichText::new("●")
                    .size(36.0)
                    .color(severity_color(severity)),
            );
            ui.add_space(8.0);
            ui.label(RichText::new(message.title).size(28.0).strong());
            ui.add_space(12.0);
            ui.add(egui::Label::new(RichText::new(message.hint).size(16.0)).wrap());

            if self.adb_conflict {
                ui.add_space(20.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(texts::ADB_CONFLICT)
                            .size(14.0)
                            .color(severity_color(Severity::Waiting)),
                    )
                    .wrap(),
                );
            }

            if self.devices.len() > 1 {
                ui.add_space(24.0);
                self.device_list(ui);
            }
        });
    }

    fn device_list(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(texts::DEVICE_LIST).size(16.0));
        ui.add_space(4.0);
        for device in &self.devices {
            let label = format!(
                "{} — {}",
                device.display_name(),
                texts::device_state_short(&device.state)
            );
            let selected = self.selected.as_deref() == Some(device.serial.as_str());
            if ui
                .radio(selected, RichText::new(label).size(16.0))
                .clicked()
            {
                self.selected = Some(device.serial.clone());
            }
        }
    }
}

/// Keeps the user's choice while that device is still connected; otherwise
/// picks the only device, or nothing when the choice is ambiguous.
fn reconcile_selection(selected: Option<String>, devices: &[DeviceInfo]) -> Option<String> {
    match selected {
        Some(serial) if devices.iter().any(|d| d.serial == serial) => Some(serial),
        _ => match devices {
            [only] => Some(only.serial.clone()),
            _ => None,
        },
    }
}

fn severity_of(state: &DeviceState) -> Severity {
    match state {
        DeviceState::Ready => Severity::Ok,
        DeviceState::Unauthorized | DeviceState::Authorizing | DeviceState::Connecting => {
            Severity::Waiting
        }
        DeviceState::Offline | DeviceState::NoPermissions | DeviceState::Other(_) => {
            Severity::Error
        }
    }
}

fn severity_color(severity: Severity) -> Color32 {
    match severity {
        Severity::Ok => Color32::from_rgb(46, 160, 67),
        Severity::Waiting => Color32::from_rgb(219, 140, 20),
        Severity::Error => Color32::from_rgb(207, 34, 46),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(serial: &str, state: DeviceState) -> DeviceInfo {
        DeviceInfo {
            serial: serial.into(),
            state,
            manufacturer: None,
            model: None,
        }
    }

    fn ready_screen(devices: Vec<DeviceInfo>) -> HomeScreen {
        let mut screen = HomeScreen::default();
        screen.apply(Event::AdbStatus(AdbStatus::Ready));
        screen.apply(Event::Devices(devices));
        screen
    }

    #[test]
    fn starts_in_starting_view() {
        assert_eq!(HomeScreen::default().view(), View::Starting);
    }

    #[test]
    fn no_device_view() {
        assert_eq!(ready_screen(vec![]).view(), View::NoDevice);
    }

    #[test]
    fn single_device_is_selected_automatically() {
        let screen = ready_screen(vec![dev("A", DeviceState::Unauthorized)]);
        assert_eq!(screen.selected.as_deref(), Some("A"));
        assert!(matches!(screen.view(), View::Device(d) if d.serial == "A"));
    }

    #[test]
    fn several_devices_require_a_choice() {
        let screen = ready_screen(vec![
            dev("A", DeviceState::Ready),
            dev("B", DeviceState::Ready),
        ]);
        assert_eq!(screen.view(), View::ChooseDevice);
    }

    #[test]
    fn choice_survives_while_device_present() {
        let mut screen = ready_screen(vec![dev("A", DeviceState::Ready)]);
        screen.apply(Event::Devices(vec![
            dev("A", DeviceState::Ready),
            dev("B", DeviceState::Ready),
        ]));
        assert_eq!(screen.selected.as_deref(), Some("A"));

        screen.apply(Event::Devices(vec![dev("B", DeviceState::Ready)]));
        assert_eq!(screen.selected.as_deref(), Some("B"));

        screen.apply(Event::Devices(vec![]));
        assert_eq!(screen.selected, None);
    }

    #[test]
    fn adb_problem_takes_precedence() {
        let mut screen = ready_screen(vec![dev("A", DeviceState::Ready)]);
        screen.apply(Event::AdbStatus(AdbStatus::Problem(
            AdbProblem::ServerFailed,
        )));
        assert_eq!(screen.view(), View::Problem(AdbProblem::ServerFailed));
    }
}
