//! Timeline: segments, cut markers, playhead, and draggable in/out handles of
//! the selected segment. Click or drag anywhere else to move the playhead.

use eframe::egui::{self, pos2, vec2, Color32, CursorIcon, Rect, Sense, Stroke, StrokeKind};
use phonecut_core::markers::{Marker, Segment};

const HEIGHT: f32 = 64.0;
const HANDLE_WIDTH: f32 = 10.0;

const BACKGROUND: Color32 = Color32::from_gray(45);
const SEGMENT: Color32 = Color32::from_rgb(52, 101, 164);
const SEGMENT_SELECTED: Color32 = Color32::from_rgb(77, 145, 230);
const SEGMENT_OFF: Color32 = Color32::from_gray(80);
const MARKER: Color32 = Color32::from_rgb(230, 70, 70);
const HANDLE: Color32 = Color32::from_rgb(255, 196, 0);

pub struct TimelineView<'a> {
    pub duration_ms: u64,
    pub position_ms: u64,
    pub markers: &'a [Marker],
    pub segments: &'a [Segment],
    pub selected: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineEdit {
    Seek(u64),
    Select(usize),
    SetStart(usize, u64),
    SetEnd(usize, u64),
}

/// Pure geometry, kept separate so it can be tested.
#[derive(Debug, Clone, Copy)]
pub struct Scale {
    left: f32,
    width: f32,
    duration_ms: u64,
}

impl Scale {
    pub fn new(left: f32, width: f32, duration_ms: u64) -> Self {
        Self {
            left,
            width: width.max(1.0),
            duration_ms: duration_ms.max(1),
        }
    }

    pub fn x(&self, ms: u64) -> f32 {
        self.left + (ms.min(self.duration_ms) as f32 / self.duration_ms as f32) * self.width
    }

    pub fn ms(&self, x: f32) -> u64 {
        let fraction = ((x - self.left) / self.width).clamp(0.0, 1.0);
        (f64::from(fraction) * self.duration_ms as f64).round() as u64
    }
}

pub fn timeline(ui: &mut egui::Ui, view: &TimelineView<'_>) -> Vec<TimelineEdit> {
    let mut edits = Vec::new();
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT), Sense::click_and_drag());
    let scale = Scale::new(rect.left(), rect.width(), view.duration_ms);
    let painter = ui.painter_at(rect.expand(2.0));

    painter.rect_filled(rect, 4.0, BACKGROUND);
    let band = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 14.0),
        pos2(rect.right(), rect.bottom() - 14.0),
    );
    for (i, segment) in view.segments.iter().enumerate() {
        let r = Rect::from_min_max(
            pos2(scale.x(segment.start_ms), band.top()),
            pos2(scale.x(segment.end_ms), band.bottom()),
        );
        let fill = if !segment.export {
            SEGMENT_OFF
        } else if view.selected == Some(i) {
            SEGMENT_SELECTED
        } else {
            SEGMENT
        };
        painter.rect_filled(r.shrink2(vec2(1.0, 0.0)), 2.0, fill);
    }
    for marker in view.markers {
        let x = scale.x(marker.time_ms);
        painter.line_segment(
            [pos2(x, rect.top() + 2.0), pos2(x, rect.bottom() - 2.0)],
            Stroke::new(2.0, MARKER),
        );
    }

    // Handles of the selected segment (registered after the background, so
    // they get the pointer first).
    let mut on_handle = false;
    if let Some((i, segment)) = view
        .selected
        .and_then(|i| view.segments.get(i).map(|s| (i, s)))
    {
        for (is_start, ms) in [(true, segment.start_ms), (false, segment.end_ms)] {
            let x = scale.x(ms);
            let handle = Rect::from_center_size(
                pos2(x, band.center().y),
                vec2(HANDLE_WIDTH, band.height() + 12.0),
            );
            let id = response.id.with((i, is_start));
            let handle_response = ui.interact(handle, id, Sense::drag());
            if handle_response.hovered() || handle_response.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                on_handle = true;
            }
            if handle_response.dragged() {
                if let Some(pointer) = handle_response.interact_pointer_pos() {
                    let ms = scale.ms(pointer.x);
                    edits.push(if is_start {
                        TimelineEdit::SetStart(i, ms)
                    } else {
                        TimelineEdit::SetEnd(i, ms)
                    });
                    edits.push(TimelineEdit::Seek(ms));
                }
            }
            painter.rect_filled(handle.shrink2(vec2(2.0, 0.0)), 2.0, HANDLE);
        }
    }

    if !on_handle && (response.clicked() || response.dragged()) {
        if let Some(pointer) = response.interact_pointer_pos() {
            let ms = scale.ms(pointer.x);
            edits.push(TimelineEdit::Seek(ms));
            if response.clicked() {
                if let Some(i) = view
                    .segments
                    .iter()
                    .position(|s| s.start_ms <= ms && ms < s.end_ms)
                {
                    edits.push(TimelineEdit::Select(i));
                }
            }
        }
    }

    // Playhead.
    let x = scale.x(view.position_ms);
    painter.line_segment(
        [pos2(x, rect.top()), pos2(x, rect.bottom())],
        Stroke::new(2.0, Color32::WHITE),
    );
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, Color32::from_gray(90)),
        StrokeKind::Inside,
    );
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_maps_both_ways() {
        let s = Scale::new(100.0, 1000.0, 60_000);
        assert_eq!(s.x(0), 100.0);
        assert_eq!(s.x(30_000), 600.0);
        assert_eq!(s.x(90_000), 1100.0, "clamped to the end");
        assert_eq!(s.ms(600.0), 30_000);
        assert_eq!(s.ms(0.0), 0, "clamped to the start");
        assert_eq!(s.ms(5000.0), 60_000);
    }

    #[test]
    fn zero_duration_does_not_divide_by_zero() {
        let s = Scale::new(0.0, 0.0, 0);
        assert!(s.x(10).is_finite());
        assert_eq!(s.ms(10.0), 1);
    }
}
