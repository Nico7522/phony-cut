//! "Files" tab: browse the phone's shared storage, copy files to the PC, and
//! drop files of the PC on the window to send them to the phone.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crossbeam_channel::Receiver;
use eframe::egui::{self, Color32, RichText};
use phonycut_core::api::{PhoneFilesProblem, RemoteEntry, TransferDirection, TransferState};
use phonycut_core::phone_files::{self, STORAGE_ROOT};

use crate::texts::{self, files as t};

const ROW_HEIGHT: f32 = 26.0;
const ORANGE: Color32 = Color32::from_rgb(214, 120, 0);

#[derive(Debug, PartialEq, Eq)]
pub enum FilesAction {
    List {
        serial: String,
        dir: String,
    },
    CopyToPc {
        serial: String,
        paths: Vec<String>,
        folder: PathBuf,
    },
    CopyToPhone {
        serial: String,
        files: Vec<PathBuf>,
        dir: String,
    },
    Cancel,
    OpenFolder(PathBuf),
    /// The PC folder chosen for copies, remembered for the next time.
    RememberFolder(PathBuf),
}

pub struct FilesScreen {
    serial: Option<String>,
    dir: String,
    entries: Vec<RemoteEntry>,
    loading: bool,
    problem: Option<PhoneFilesProblem>,
    /// Phone paths ticked, in any folder, with their size (0 for a folder):
    /// the selection stays while moving between folders.
    selected: BTreeMap<String, u64>,
    transfer: Option<TransferState>,
    /// Folder dialog open, and the phone paths to copy once a folder is chosen.
    picker: Option<(Receiver<Option<PathBuf>>, Vec<String>)>,
}

impl Default for FilesScreen {
    fn default() -> Self {
        Self {
            serial: None,
            dir: STORAGE_ROOT.to_owned(),
            entries: Vec::new(),
            loading: false,
            problem: None,
            selected: BTreeMap::new(),
            transfer: None,
            picker: None,
        }
    }
}

impl FilesScreen {
    /// Listing of a folder received.
    pub fn apply_listing(
        &mut self,
        dir: String,
        result: Result<Vec<RemoteEntry>, PhoneFilesProblem>,
    ) {
        if dir != self.dir {
            return; // an older request
        }
        self.loading = false;
        match result {
            Ok(entries) => {
                self.entries = entries;
                self.problem = None;
                // Ticked items of this folder that are gone are unticked.
                let present: BTreeSet<&str> =
                    self.entries.iter().map(|e| e.path.as_str()).collect();
                let here = self.dir.trim_end_matches('/');
                self.selected.retain(|p, _| {
                    p.rsplit_once('/').map(|(d, _)| d) != Some(here) || present.contains(p.as_str())
                });
            }
            Err(problem) => {
                self.entries.clear();
                self.problem = Some(problem);
            }
        }
    }

    /// Copy progress. Returns a listing to ask for once files reached the
    /// phone (they show up in the open folder).
    pub fn apply_transfer(&mut self, state: TransferState) -> Option<FilesAction> {
        let refresh = matches!(
            state,
            TransferState::Done {
                direction: TransferDirection::ToPhone,
                ..
            } | TransferState::Failed {
                direction: TransferDirection::ToPhone,
                ..
            } | TransferState::Cancelled {
                direction: TransferDirection::ToPhone
            }
        );
        if matches!(state, TransferState::Done { .. }) {
            self.selected.clear();
        }
        self.transfer = Some(state);
        refresh.then(|| self.list_action()).flatten()
    }

    fn is_copying(&self) -> bool {
        matches!(self.transfer, Some(TransferState::Running { .. })) || self.picker.is_some()
    }

    fn list_action(&mut self) -> Option<FilesAction> {
        let serial = self.serial.clone()?;
        self.loading = true;
        Some(FilesAction::List {
            serial,
            dir: self.dir.clone(),
        })
    }

    fn open(&mut self, dir: String) -> Option<FilesAction> {
        self.dir = dir;
        self.entries.clear();
        self.problem = None;
        self.list_action()
    }

    /// The tab is shown: the open folder is read again.
    pub fn opened(&mut self) -> Option<FilesAction> {
        self.list_action()
    }

    /// The phone in use; a new phone starts at the storage root.
    fn set_phone(&mut self, serial: Option<&str>) -> Option<FilesAction> {
        if self.serial.as_deref() == serial {
            return None;
        }
        self.serial = serial.map(str::to_owned);
        self.transfer = None;
        if self.serial.is_some() {
            self.open(STORAGE_ROOT.to_owned())
        } else {
            self.entries.clear();
            self.selected.clear();
            None
        }
    }

    /// Opens the folder dialog on its own thread.
    fn pick_folder(&mut self, ctx: &egui::Context, start: &Path, paths: Vec<String>) {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let start = start.to_path_buf();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("files-folder-dialog".into())
            .spawn(move || {
                let mut dialog = rfd::FileDialog::new().set_title(t::pick_folder_title());
                if let Some(existing) = start.ancestors().find(|p| p.is_dir()) {
                    dialog = dialog.set_directory(existing);
                }
                let _ = tx.send(dialog.pick_folder());
                ctx.request_repaint();
            });
        match spawned {
            Ok(_) => self.picker = Some((rx, paths)),
            Err(err) => tracing::error!(%err, "cannot open folder dialog"),
        }
    }

    fn poll_picker(&mut self, actions: &mut Vec<FilesAction>) {
        let Some((rx, _)) = &self.picker else { return };
        let answer = match rx.try_recv() {
            Ok(answer) => answer,
            Err(crossbeam_channel::TryRecvError::Empty) => return,
            Err(crossbeam_channel::TryRecvError::Disconnected) => None,
        };
        let Some((_, paths)) = self.picker.take() else {
            return;
        };
        if let (Some(folder), Some(serial)) = (answer, self.serial.clone()) {
            actions.push(FilesAction::RememberFolder(folder.clone()));
            actions.push(FilesAction::CopyToPc {
                serial,
                paths,
                folder,
            });
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        serial: Option<&str>,
        last_folder: &Path,
    ) -> Vec<FilesAction> {
        let mut actions = Vec::new();
        actions.extend(self.set_phone(serial));
        self.poll_picker(&mut actions);
        let ctx = ui.ctx().clone();

        let Some(serial) = self.serial.clone() else {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.heading(t::no_phone());
                    ui.label(RichText::new(t::no_phone_hint()).weak());
                });
            });
            return actions;
        };

        // Files of the PC dropped on the window go to the open folder.
        let (dropped, hovering) = ui.input(|i| {
            let dropped: Vec<PathBuf> = i
                .raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect();
            (dropped, !i.raw.hovered_files.is_empty())
        });
        if !dropped.is_empty() {
            if self.is_copying() {
                tracing::info!("drop ignored: a copy is running");
            } else {
                actions.push(FilesAction::CopyToPhone {
                    serial: serial.clone(),
                    files: dropped,
                    dir: self.dir.clone(),
                });
            }
        }

        egui::Panel::bottom("files-actions").show(ui, |ui| {
            ui.add_space(6.0);
            self.transfer_line(ui, &mut actions);
            ui.horizontal(|ui| {
                let count = self.selected.len();
                let bytes: u64 = self.selected.values().sum();
                let button = egui::Button::new(RichText::new(t::copy_to_pc()).size(17.0))
                    .min_size(egui::vec2(200.0, 40.0));
                if ui
                    .add_enabled(count > 0 && !self.is_copying(), button)
                    .on_hover_text(t::copy_to_pc_hint())
                    .clicked()
                {
                    let paths = without_nested(self.selected.keys());
                    self.pick_folder(&ctx, last_folder, paths);
                }
                ui.label(t::selection(count, bytes));
                if count > 0 && !self.is_copying() && ui.button(t::clear_selection()).clicked() {
                    self.selected.clear();
                }
            });
            ui.label(RichText::new(t::drop_hint()).weak());
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, dir) in [
                    (t::place_storage(), STORAGE_ROOT.to_owned()),
                    (t::place_camera(), format!("{STORAGE_ROOT}/DCIM/Camera")),
                    (
                        t::place_screenshots(),
                        format!("{STORAGE_ROOT}/DCIM/Screenshots"),
                    ),
                    (t::place_downloads(), format!("{STORAGE_ROOT}/Download")),
                    (t::place_music(), format!("{STORAGE_ROOT}/Music")),
                ] {
                    if ui.selectable_label(self.dir == dir, label).clicked() {
                        actions.extend(self.open(dir));
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let parent = phone_files::parent(&self.dir);
                if ui
                    .add_enabled(parent.is_some(), egui::Button::new(t::up()))
                    .clicked()
                {
                    if let Some(parent) = parent {
                        actions.extend(self.open(parent));
                    }
                }
                if ui.button(t::refresh()).clicked() {
                    actions.extend(self.list_action());
                }
                ui.label(RichText::new(t::shown_path(&self.dir)).monospace());
                if self.loading {
                    ui.spinner();
                }
            });
            ui.separator();

            if hovering {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.heading(
                        RichText::new(t::drop_here(&self.dir))
                            .color(Color32::from_rgb(77, 145, 230)),
                    );
                });
                return;
            }
            if let Some(problem) = self.problem {
                ui.colored_label(ORANGE, t::list_problem(problem));
                return;
            }
            if self.entries.is_empty() && !self.loading {
                ui.label(RichText::new(t::empty_folder()).weak());
                return;
            }
            self.list(ui, &mut actions);
        });
        actions
    }

    fn list(&mut self, ui: &mut egui::Ui, actions: &mut Vec<FilesAction>) {
        let offset = local_offset_secs();
        let mut open = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, ROW_HEIGHT, self.entries.len(), |ui, rows| {
                for entry in &self.entries[rows] {
                    ui.horizontal(|ui| {
                        ui.set_min_height(ROW_HEIGHT);
                        let size = if entry.is_dir { 0 } else { entry.size };
                        let mut ticked = self.selected.contains_key(&entry.path);
                        if ui.checkbox(&mut ticked, "").changed() {
                            if ticked {
                                self.selected.insert(entry.path.clone(), size);
                            } else {
                                self.selected.remove(&entry.path);
                            }
                        }
                        let icon = if entry.is_dir { "📁" } else { "📄" };
                        let name = RichText::new(format!("{icon}  {}", entry.name)).size(15.0);
                        let response = ui
                            .add(
                                egui::Label::new(name)
                                    .truncate()
                                    .sense(egui::Sense::click()),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if response.clicked() {
                            if entry.is_dir {
                                open = Some(entry.path.clone());
                            } else if self.selected.remove(&entry.path).is_none() {
                                self.selected.insert(entry.path.clone(), size);
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(modified) = entry.modified {
                                ui.label(
                                    RichText::new(date_text(modified, offset))
                                        .weak()
                                        .monospace(),
                                );
                            }
                            if !entry.is_dir {
                                ui.add_space(12.0);
                                ui.label(RichText::new(texts::file_size(entry.size)).weak());
                            }
                        });
                    });
                }
            });
        if let Some(dir) = open {
            actions.extend(self.open(dir));
        }
    }

    fn transfer_line(&mut self, ui: &mut egui::Ui, actions: &mut Vec<FilesAction>) {
        let Some(state) = &self.transfer else { return };
        match state {
            TransferState::Running {
                direction,
                done,
                total,
                file,
            } => {
                ui.horizontal(|ui| {
                    let fraction = if *total > 0 {
                        *done as f32 / *total as f32
                    } else {
                        0.0
                    };
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .desired_width(260.0)
                            .text(t::progress(*direction, *done, *total)),
                    );
                    ui.add(egui::Label::new(RichText::new(file).weak()).truncate());
                    if ui.button(t::cancel()).clicked() {
                        actions.push(FilesAction::Cancel);
                    }
                });
            }
            TransferState::Done {
                direction,
                count,
                folder,
            } => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t::done(*direction, *count)).strong());
                    if let Some(folder) = folder {
                        if ui.button(texts::open_folder()).clicked() {
                            actions.push(FilesAction::OpenFolder(folder.clone()));
                        }
                    }
                });
            }
            TransferState::Cancelled { .. } => {
                ui.label(RichText::new(t::cancelled()).weak());
            }
            TransferState::Failed { problem, .. } => {
                ui.colored_label(ORANGE, t::transfer_problem(*problem));
            }
        }
        ui.add_space(4.0);
    }
}

/// The ticked paths, without those inside a ticked folder (copied with it).
fn without_nested<'a>(paths: impl Iterator<Item = &'a String> + Clone) -> Vec<String> {
    paths
        .clone()
        .filter(|path| {
            !paths
                .clone()
                .any(|other| path.starts_with(&format!("{other}/")))
        })
        .cloned()
        .collect()
}

/// Local time minus UTC, in seconds (from the system clock and the local
/// wall-clock time).
fn local_offset_secs() -> i64 {
    let now = phonycut_core::platform::local_now();
    let local = days_from_civil(
        i64::from(now.year),
        u32::from(now.month),
        u32::from(now.day),
    ) * 86_400
        + i64::from(now.hour) * 3600
        + i64::from(now.minute) * 60
        + i64::from(now.second);
    let utc = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    // Rounded to the quarter hour (time zones), the seconds in between do not count.
    ((local - utc) as f64 / 900.0).round() as i64 * 900
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let month = i64::from(month);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// (year, month, day) of a day count since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `21/11/2026 13:05` (French) or `2026-11-21 13:05`.
fn date_text(unix: u64, offset: i64) -> String {
    let local = i64::try_from(unix).unwrap_or(0) + offset;
    let (year, month, day) = civil_from_days(local.div_euclid(86_400));
    let secs = local.rem_euclid(86_400);
    texts::date_time(year, month, day, secs / 3600, secs % 3600 / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_inside_a_ticked_folder_are_copied_once() {
        let ticked: Vec<String> = [
            "/sdcard/DCIM/Camera",
            "/sdcard/DCIM/Camera/a.jpg",
            "/sdcard/DCIM/Screenshots/b.png",
            "/sdcard/DCIM/CameraRoll.jpg",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            without_nested(ticked.iter()),
            [
                "/sdcard/DCIM/Camera",
                "/sdcard/DCIM/Screenshots/b.png",
                "/sdcard/DCIM/CameraRoll.jpg"
            ]
        );
    }

    #[test]
    fn civil_dates_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        let days = days_from_civil(2026, 11, 21);
        assert_eq!(civil_from_days(days), (2026, 11, 21));
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        // 1_750_304_540 = 2025-06-19 03:42:20 UTC
        assert_eq!(civil_from_days(1_750_304_540 / 86_400), (2025, 6, 19));
    }
}
