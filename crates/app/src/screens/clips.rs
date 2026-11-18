//! "Mes clips": list of recordings, player, timeline with cut markers,
//! segment editing (in/out points, names, selection) and export.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, TextureHandle};
use phonecut_core::api::{
    ExportMode, ExportRequest, ExportState, LoadedVideo, PlayerEvent, RecordingEntry, VideoFrame,
};
use phonecut_core::markers::{default_segments, CutFile, Segment, MIN_SEGMENT_MS};

use crate::texts;
use crate::widgets::timeline::{timeline, TimelineEdit, TimelineView};
use crate::widgets::video;

/// Edits are written to disk once the user pauses for this long.
const SAVE_DELAY: Duration = Duration::from_millis(500);
const RED: Color32 = Color32::from_rgb(207, 34, 46);

/// What the screen asks the core to do.
#[derive(Debug, PartialEq)]
pub enum ClipsAction {
    Refresh,
    Open(PathBuf),
    Play,
    Pause,
    Seek(Duration),
    Save(PathBuf, CutFile),
    Export(ExportRequest),
    CancelExport,
    OpenFolder(PathBuf),
}

/// The video being edited.
struct Editing {
    path: PathBuf,
    duration_ms: u64,
    cuts: CutFile,
    segments: Vec<Segment>,
    selected: Option<usize>,
    /// Unsaved edits since this instant.
    dirty_since: Option<Instant>,
}

impl Editing {
    fn from_loaded(loaded: LoadedVideo) -> Self {
        let duration_ms = u64::try_from(loaded.duration.as_millis()).unwrap_or(u64::MAX);
        let segments = loaded.cuts.segments_or_default(duration_ms);
        Self {
            path: loaded.path,
            duration_ms,
            cuts: loaded.cuts,
            selected: (!segments.is_empty()).then_some(0),
            segments,
            dirty_since: None,
        }
    }

    fn changed(&mut self) {
        self.dirty_since.get_or_insert_with(Instant::now);
    }

    fn save_action(&mut self) -> ClipsAction {
        self.dirty_since = None;
        let mut cuts = self.cuts.clone();
        cuts.segments = Some(self.segments.clone());
        ClipsAction::Save(self.path.clone(), cuts)
    }

    fn selected_count(&self) -> usize {
        self.segments.iter().filter(|s| s.export).count()
    }
}

#[derive(Default)]
pub struct ClipsScreen {
    recordings: Vec<RecordingEntry>,
    listed: bool,
    editing: Option<Editing>,
    /// Requested, waiting for the player to load it.
    opening: Option<PathBuf>,
    failed: Option<PathBuf>,
    texture: Option<TextureHandle>,
    position_ms: u64,
    playing: bool,
    mode: ExportMode,
    export: Option<ExportState>,
    exporting: bool,
    notice: Option<&'static str>,
}

impl ClipsScreen {
    /// Needs the list of recordings (first visit).
    pub fn needs_listing(&self) -> bool {
        !self.listed
    }

    /// The list is out of date (new recording, other folder).
    pub fn invalidate(&mut self) {
        self.listed = false;
    }

    pub fn set_recordings(&mut self, recordings: Vec<RecordingEntry>) {
        self.listed = true;
        self.recordings = recordings;
    }

    /// Asks to open `path`; returns the actions (save pending edits first).
    pub fn open(&mut self, path: PathBuf) -> Vec<ClipsAction> {
        let mut actions = self.flush();
        if self.editing.as_ref().is_some_and(|e| e.path == path) {
            return actions;
        }
        self.editing = None;
        self.texture = None;
        self.failed = None;
        self.export = None;
        self.position_ms = 0;
        self.playing = false;
        self.opening = Some(path.clone());
        actions.push(ClipsAction::Open(path));
        actions
    }

    /// Pending edits, to save now (closing, switching video).
    pub fn flush(&mut self) -> Vec<ClipsAction> {
        match &mut self.editing {
            Some(e) if e.dirty_since.is_some() => vec![e.save_action()],
            _ => Vec::new(),
        }
    }

    pub fn apply_player(&mut self, event: PlayerEvent) {
        match event {
            PlayerEvent::Loaded(loaded) => {
                if self.opening.as_ref() == Some(&loaded.path) {
                    self.opening = None;
                    self.editing = Some(Editing::from_loaded(loaded));
                }
            }
            PlayerEvent::Position { time, playing } => {
                self.position_ms = u64::try_from(time.as_millis()).unwrap_or(0);
                self.playing = playing;
            }
            PlayerEvent::Ended => self.playing = false,
            PlayerEvent::Failed { path } => {
                self.opening = None;
                self.editing = None;
                self.failed = Some(path);
            }
        }
    }

    pub fn apply_export(&mut self, state: ExportState) {
        self.exporting = matches!(state, ExportState::Progress { .. });
        self.export = Some(state);
    }

    pub fn cuts_save_failed(&mut self) {
        self.notice = Some(texts::cuts_save_failed());
    }

    pub fn show_frame(&mut self, ctx: &egui::Context, frame: &VideoFrame) {
        if self.editing.is_some() {
            video::upload(ctx, &mut self.texture, "clip-player", frame);
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Vec<ClipsAction> {
        let mut actions = Vec::new();
        egui::Panel::left("recordings")
            .resizable(true)
            .min_size(270.0)
            .default_size(300.0)
            .show(ui, |ui| self.recordings_list(ui, &mut actions));
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(notice) = self.notice {
                ui.colored_label(RED, notice);
            }
            if self.editing.is_some() {
                self.editor(ui, &mut actions);
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.3);
                    let message = if self.opening.is_some() {
                        texts::clips_loading()
                    } else if self.failed.is_some() {
                        texts::player_failed()
                    } else if self.recordings.is_empty() {
                        texts::clips_empty()
                    } else {
                        texts::clips_pick()
                    };
                    ui.add(egui::Label::new(RichText::new(message).size(18.0)).wrap());
                });
            }
        });

        // Debounced save of edits.
        if let Some(editing) = &mut self.editing {
            if editing
                .dirty_since
                .is_some_and(|since| since.elapsed() >= SAVE_DELAY)
            {
                actions.push(editing.save_action());
            } else if editing.dirty_since.is_some() {
                ui.ctx().request_repaint_after(SAVE_DELAY);
            }
        }
        actions
    }

    fn recordings_list(&mut self, ui: &mut egui::Ui, actions: &mut Vec<ClipsAction>) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading(texts::clips_list_title());
            if ui.button(texts::clips_refresh()).clicked() {
                actions.push(ClipsAction::Refresh);
            }
        });
        ui.add_space(6.0);
        let current = self
            .editing
            .as_ref()
            .map(|e| e.path.clone())
            .or_else(|| self.opening.clone());
        let mut open = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for entry in &self.recordings {
                let name = file_stem(&entry.path);
                let selected = current.as_deref() == Some(entry.path.as_path());
                let label = RichText::new(format!(
                    "{name}\n{}",
                    texts::recording_details(entry.bytes, entry.cuts)
                ));
                if ui.selectable_label(selected, label).clicked() && !selected {
                    open = Some(entry.path.clone());
                }
                ui.add_space(2.0);
            }
        });
        if let Some(path) = open {
            actions.extend(self.open(path));
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui, actions: &mut Vec<ClipsAction>) {
        let Some(editing) = &mut self.editing else {
            return;
        };

        // Keyboard: space = play/pause (unless typing a name).
        let typing = ui.ctx().memory(|m| m.focused().is_some());
        if !typing && ui.input(|i| i.key_pressed(egui::Key::Space)) {
            actions.push(if self.playing {
                ClipsAction::Pause
            } else {
                ClipsAction::Play
            });
        }

        ui.label(RichText::new(file_stem(&editing.path)).heading());

        // Video, leaving room for the controls below (they grow with the
        // number of clips, up to the height of the scrollable list).
        let rows = editing.segments.len().min(5) as f32;
        let controls = 215.0 + 26.0 * rows;
        let video_height = (ui.available_height() - controls).max(160.0);
        ui.allocate_ui(egui::vec2(ui.available_width(), video_height), |ui| {
            ui.set_min_height(video_height);
            if let Some(texture) = &self.texture {
                video::fit(ui, texture);
            }
        });

        // Transport.
        ui.horizontal(|ui| {
            let (label, action) = if self.playing {
                (texts::pause(), ClipsAction::Pause)
            } else {
                (texts::play(), ClipsAction::Play)
            };
            let button = egui::Button::new(RichText::new(label).size(16.0))
                .min_size(egui::vec2(110.0, 32.0));
            if ui.add(button).on_hover_text(texts::play_hint()).clicked() {
                actions.push(action);
            }
            ui.monospace(format!(
                "{} / {}",
                texts::duration(Duration::from_millis(self.position_ms)),
                texts::duration(Duration::from_millis(editing.duration_ms))
            ));
            ui.separator();
            if ui
                .button(texts::split_here())
                .on_hover_text(texts::split_hint())
                .clicked()
            {
                if let Some(i) = split_at(&mut editing.segments, self.position_ms) {
                    editing.selected = Some(i + 1);
                    editing.changed();
                }
            }
            if ui.button(texts::reset_segments()).clicked() {
                editing.segments = default_segments(&editing.cuts.markers, editing.duration_ms);
                editing.selected = (!editing.segments.is_empty()).then_some(0);
                editing.changed();
            }
        });

        // Timeline.
        let edits = timeline(
            ui,
            &TimelineView {
                duration_ms: editing.duration_ms,
                position_ms: self.position_ms,
                markers: &editing.cuts.markers,
                segments: &editing.segments,
                selected: editing.selected,
            },
        );
        for edit in edits {
            match edit {
                TimelineEdit::Seek(ms) => {
                    self.position_ms = ms;
                    actions.push(ClipsAction::Seek(Duration::from_millis(ms)));
                }
                TimelineEdit::Select(i) => editing.selected = Some(i),
                TimelineEdit::SetStart(i, ms) => {
                    if let Some(segment) = editing.segments.get_mut(i) {
                        segment.set_start(ms);
                        editing.changed();
                    }
                }
                TimelineEdit::SetEnd(i, ms) => {
                    if let Some(segment) = editing.segments.get_mut(i) {
                        segment.set_end(ms, editing.duration_ms);
                        editing.changed();
                    }
                }
            }
        }

        // Segments.
        ui.add_space(6.0);
        ui.label(RichText::new(texts::segments_title()).strong())
            .on_hover_text(texts::segments_hint());
        let position = self.position_ms;
        egui::ScrollArea::vertical()
            .max_height(150.0)
            .show(ui, |ui| {
                let duration_ms = editing.duration_ms;
                let mut changed = false;
                for (i, segment) in editing.segments.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut segment.export, "").changed();
                        let selected = editing.selected == Some(i);
                        if ui
                            .selectable_label(selected, format!("{:02}", i + 1))
                            .clicked()
                        {
                            editing.selected = Some(i);
                        }
                        let name =
                            egui::TextEdit::singleline(&mut segment.name).desired_width(180.0);
                        let response = ui.add(name);
                        if response.gained_focus() {
                            editing.selected = Some(i);
                        }
                        changed |= response.changed();
                        ui.monospace(texts::segment_range(segment.start_ms, segment.end_ms));
                        if ui.button(texts::segment_go()).clicked() {
                            editing.selected = Some(i);
                            actions
                                .push(ClipsAction::Seek(Duration::from_millis(segment.start_ms)));
                        }
                        if ui.button(texts::segment_set_start()).clicked() {
                            segment.set_start(position);
                            changed = true;
                        }
                        if ui.button(texts::segment_set_end()).clicked() {
                            segment.set_end(position, duration_ms);
                            changed = true;
                        }
                    });
                }
                if changed {
                    editing.changed();
                }
            });

        // Export.
        ui.separator();
        let count = editing.selected_count();
        let request = ExportRequest {
            video: editing.path.clone(),
            segments: editing.segments.clone(),
            mode: self.mode,
        };
        ui.horizontal(|ui| {
            ui.label(texts::export_mode());
            ui.radio_value(&mut self.mode, ExportMode::Fast, texts::export_fast())
                .on_hover_text(texts::export_fast_hint());
            ui.radio_value(&mut self.mode, ExportMode::Precise, texts::export_precise())
                .on_hover_text(texts::export_precise_hint());
            ui.add_space(12.0);
            if self.exporting {
                if ui.button(texts::export_cancel()).clicked() {
                    actions.push(ClipsAction::CancelExport);
                }
            } else {
                let button = egui::Button::new(
                    RichText::new(texts::export_button(count))
                        .size(16.0)
                        .color(Color32::WHITE),
                )
                .fill(Color32::from_rgb(46, 125, 50))
                .min_size(egui::vec2(180.0, 32.0));
                if ui.add_enabled(count > 0, button).clicked() {
                    self.exporting = true;
                    self.export = None;
                    actions.push(ClipsAction::Export(request));
                }
            }
        });
        ui.label(
            RichText::new(match self.mode {
                ExportMode::Fast => texts::export_fast_hint(),
                ExportMode::Precise => texts::export_precise_hint(),
            })
            .small(),
        );
        self.export_status(ui, actions);
    }

    fn export_status(&self, ui: &mut egui::Ui, actions: &mut Vec<ClipsAction>) {
        match &self.export {
            Some(ExportState::Progress { fraction, .. }) => {
                ui.add(
                    egui::ProgressBar::new(*fraction)
                        .show_percentage()
                        .text(texts::export_running()),
                );
            }
            Some(ExportState::Finished { files, folder }) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(texts::export_done(files.len())).strong());
                    if ui.button(texts::open_folder()).clicked() {
                        actions.push(ClipsAction::OpenFolder(folder.clone()));
                    }
                });
            }
            Some(ExportState::Cancelled) => {
                ui.label(texts::export_cancelled());
            }
            Some(ExportState::Failed { problem }) => {
                ui.colored_label(RED, texts::export_problem(*problem));
            }
            None => {}
        }
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

/// Splits the segment containing `ms` in two; returns its index. Both parts
/// must be at least [`MIN_SEGMENT_MS`] long.
pub fn split_at(segments: &mut Vec<Segment>, ms: u64) -> Option<usize> {
    let i = segments
        .iter()
        .position(|s| s.start_ms + MIN_SEGMENT_MS <= ms && ms + MIN_SEGMENT_MS <= s.end_ms)?;
    let second = Segment {
        name: texts::continued(&segments[i].name),
        start_ms: ms,
        end_ms: segments[i].end_ms,
        export: segments[i].export,
    };
    segments[i].end_ms = ms;
    segments.insert(i + 1, second);
    Some(i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonecut_core::markers::Marker;

    fn loaded(markers: &[u64]) -> LoadedVideo {
        LoadedVideo {
            path: PathBuf::from("v.mkv"),
            duration: Duration::from_secs(10),
            width: 10,
            height: 10,
            cuts: CutFile {
                markers: markers.iter().map(|&time_ms| Marker { time_ms }).collect(),
                ..CutFile::default()
            },
        }
    }

    #[test]
    fn split_in_the_middle() {
        let mut segments = default_segments(&[], 10_000);
        assert_eq!(split_at(&mut segments, 4_000), Some(0));
        let ranges: Vec<_> = segments.iter().map(|s| (s.start_ms, s.end_ms)).collect();
        assert_eq!(ranges, vec![(0, 4_000), (4_000, 10_000)]);
        assert!(
            segments[1].name.starts_with("Clip 1 ("),
            "{}",
            segments[1].name
        );
    }

    #[test]
    fn split_too_close_to_an_edge_is_refused() {
        let mut segments = default_segments(&[], 10_000);
        assert_eq!(split_at(&mut segments, 200), None);
        assert_eq!(split_at(&mut segments, 9_900), None);
        assert_eq!(segments.len(), 1);
    }

    #[test]
    fn open_then_load_creates_segments_from_markers() {
        let mut screen = ClipsScreen::default();
        let actions = screen.open(PathBuf::from("v.mkv"));
        assert_eq!(actions, vec![ClipsAction::Open(PathBuf::from("v.mkv"))]);
        // A late event for another file is ignored.
        let mut other = loaded(&[]);
        other.path = PathBuf::from("other.mkv");
        screen.apply_player(PlayerEvent::Loaded(other));
        assert!(screen.editing.is_none());

        screen.apply_player(PlayerEvent::Loaded(loaded(&[3_000, 6_000])));
        let editing = screen.editing.as_ref().unwrap();
        assert_eq!(editing.segments.len(), 3);
        assert_eq!(editing.selected, Some(0));
        assert_eq!(editing.selected_count(), 3);
    }

    #[test]
    fn edits_are_saved_when_switching_video() {
        let mut screen = ClipsScreen::default();
        screen.open(PathBuf::from("v.mkv"));
        screen.apply_player(PlayerEvent::Loaded(loaded(&[3_000])));
        let editing = screen.editing.as_mut().unwrap();
        editing.segments[0].name = "Intro".into();
        editing.changed();

        let actions = screen.open(PathBuf::from("w.mkv"));
        let ClipsAction::Save(path, cuts) = &actions[0] else {
            panic!("expected a save first: {actions:?}");
        };
        assert_eq!(path, &PathBuf::from("v.mkv"));
        assert_eq!(cuts.segments.as_ref().unwrap()[0].name, "Intro");
        assert_eq!(
            cuts.markers,
            vec![Marker { time_ms: 3_000 }],
            "markers kept"
        );
        assert_eq!(actions[1], ClipsAction::Open(PathBuf::from("w.mkv")));
    }

    #[test]
    fn export_state_tracking() {
        let mut screen = ClipsScreen::default();
        screen.apply_export(ExportState::Progress {
            clip: 0,
            clips: 2,
            fraction: 0.2,
        });
        assert!(screen.exporting);
        screen.apply_export(ExportState::Cancelled);
        assert!(!screen.exporting);
    }
}
