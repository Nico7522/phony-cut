//! Turns mouse and keyboard events over the phone picture into input for the
//! phone (scrcpy control): left button = finger, right = Back, middle = Home,
//! wheel = scroll; typed text and editing keys when the picture has the
//! keyboard focus (click on it; click elsewhere to give the keyboard back).

use eframe::egui::{
    self, Event, EventFilter, Id, ImeEvent, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2,
    Rect, Sense,
};
use phonycut_core::api::{PhoneButton, PhoneInput, PhoneKey, TouchPhase};

/// Points per wheel tick when the system reports pixels (touchpads).
const POINTS_PER_TICK: f32 = 50.0;

#[derive(Default)]
pub struct PhoneControl {
    /// The left button went down on the picture and is still held.
    touching: bool,
    /// Last position sent while touching (to finish a drag that leaves the picture).
    last: Option<(u32, u32)>,
}

impl PhoneControl {
    /// Processes this frame's events for the picture drawn in `image`
    /// (`frame` = video size in pixels). `id` identifies the picture for
    /// keyboard focus.
    pub fn handle(
        &mut self,
        ui: &egui::Ui,
        id: Id,
        image: Rect,
        frame: (u32, u32),
    ) -> Vec<PhoneInput> {
        let mut inputs = Vec::new();
        let (fw, fh) = frame;
        if fw == 0 || fh == 0 {
            return inputs;
        }
        let touch = |phase, (x, y): (u32, u32)| PhoneInput::Touch {
            phase,
            x,
            y,
            frame_width: fw,
            frame_height: fh,
        };
        // Register the picture as a focusable widget every frame: egui drops
        // the keyboard focus of an id that no widget used during the frame.
        let _ = ui.interact(image, id, Sense::click_and_drag());
        let focused = ui.memory(|m| m.has_focus(id));
        let hover = ui.input(|i| i.pointer.hover_pos());
        let events = ui.input(|i| i.events.clone());

        for event in &events {
            match event {
                Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    ..
                } => {
                    let inside = image.contains(*pos);
                    match (button, pressed) {
                        (PointerButton::Primary, true) if inside => {
                            let at = to_frame(*pos, image, frame);
                            self.touching = true;
                            self.last = Some(at);
                            inputs.push(touch(TouchPhase::Down, at));
                            ui.memory_mut(|m| m.request_focus(id));
                        }
                        (PointerButton::Primary, true) => {
                            // Clicking elsewhere gives the keyboard back to PhonyCut.
                            if focused {
                                ui.memory_mut(|m| m.surrender_focus(id));
                            }
                        }
                        (PointerButton::Primary, false) if self.touching => {
                            let at = if inside {
                                to_frame(*pos, image, frame)
                            } else {
                                self.last.unwrap_or_else(|| to_frame(*pos, image, frame))
                            };
                            self.touching = false;
                            inputs.push(touch(TouchPhase::Up, at));
                        }
                        (PointerButton::Secondary, true) if inside => {
                            inputs.push(PhoneInput::Button(PhoneButton::Back));
                        }
                        (PointerButton::Middle, true) if inside => {
                            inputs.push(PhoneInput::Button(PhoneButton::Home));
                        }
                        _ => {}
                    }
                }
                Event::PointerMoved(pos) if self.touching => {
                    let at = to_frame(*pos, image, frame);
                    if self.last != Some(at) {
                        self.last = Some(at);
                        inputs.push(touch(TouchPhase::Move, at));
                    }
                }
                Event::PointerGone if self.touching => {
                    self.touching = false;
                    if let Some(at) = self.last {
                        inputs.push(touch(TouchPhase::Up, at));
                    }
                }
                Event::MouseWheel { unit, delta, .. } => {
                    if let Some(pos) = hover.filter(|p| image.contains(*p)) {
                        let (horizontal, vertical) = wheel_ticks(*unit, delta.x, delta.y);
                        if horizontal != 0.0 || vertical != 0.0 {
                            let (x, y) = to_frame(pos, image, frame);
                            inputs.push(PhoneInput::Scroll {
                                x,
                                y,
                                frame_width: fw,
                                frame_height: fh,
                                horizontal,
                                vertical,
                            });
                        }
                    }
                }
                _ if focused => inputs.extend(keyboard(event)),
                _ => {}
            }
        }

        if ui.memory(|m| m.has_focus(id)) {
            // Keep Tab, arrows and Escape for the phone instead of egui's
            // focus navigation.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                );
            });
        }
        inputs
    }
}

/// Keyboard events forwarded while the picture has the focus.
fn keyboard(event: &Event) -> Vec<PhoneInput> {
    match event {
        // Typed characters, with the PC's layout and accents applied.
        Event::Text(text) => vec![PhoneInput::Text(text.clone())],
        // Text composed with an input method (IME).
        Event::Ime(ImeEvent::Commit(text)) if !text.is_empty() => {
            vec![PhoneInput::Text(text.clone())]
        }
        // Ctrl+V pastes the PC clipboard into the phone.
        Event::Paste(text) => vec![PhoneInput::Text(text.clone())],
        // Ctrl+C / Ctrl+X act on the phone's own selection.
        Event::Copy => shortcut('c'),
        Event::Cut => shortcut('x'),
        Event::Key {
            key,
            pressed,
            repeat,
            modifiers,
            ..
        } => key_input(*key, *pressed, *repeat, *modifiers)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn shortcut(letter: char) -> Vec<PhoneInput> {
    [true, false]
        .into_iter()
        .map(|down| PhoneInput::Key {
            key: PhoneKey::Letter(letter),
            down,
            repeat: 0,
            shift: false,
            ctrl: true,
            alt: false,
        })
        .collect()
}

/// A key press for the phone, or `None` when the key is sent as text instead
/// (letters, digits, space, punctuation) or is not handled.
pub fn key_input(
    key: Key,
    pressed: bool,
    repeat: bool,
    modifiers: Modifiers,
) -> Option<PhoneInput> {
    let phone_key = match key {
        Key::Enter => PhoneKey::Enter,
        Key::Backspace => PhoneKey::Backspace,
        Key::Delete => PhoneKey::Delete,
        Key::Tab => PhoneKey::Tab,
        Key::Escape => PhoneKey::Escape,
        Key::ArrowUp => PhoneKey::Up,
        Key::ArrowDown => PhoneKey::Down,
        Key::ArrowLeft => PhoneKey::Left,
        Key::ArrowRight => PhoneKey::Right,
        Key::Home => PhoneKey::Home,
        Key::End => PhoneKey::End,
        Key::PageUp => PhoneKey::PageUp,
        Key::PageDown => PhoneKey::PageDown,
        _ => {
            // Letters only as shortcuts (Ctrl+A…). AltGr is reported as
            // Ctrl+Alt and produces text (€, @…): leave it to the text event.
            let shortcut = modifiers.ctrl != modifiers.alt;
            let name = key.name();
            let letter = name
                .chars()
                .next()
                .filter(|c| name.len() == 1 && c.is_ascii_alphabetic())?;
            if !shortcut {
                return None;
            }
            PhoneKey::Letter(letter.to_ascii_lowercase())
        }
    };
    Some(PhoneInput::Key {
        key: phone_key,
        down: pressed,
        repeat: u32::from(repeat),
        shift: modifiers.shift,
        ctrl: modifiers.ctrl,
        alt: modifiers.alt,
    })
}

/// Pixel of the video frame under `pos`, clamped to the frame.
pub fn to_frame(pos: Pos2, image: Rect, (fw, fh): (u32, u32)) -> (u32, u32) {
    let rel_x = ((pos.x - image.min.x) / image.width().max(1.0)).clamp(0.0, 1.0);
    let rel_y = ((pos.y - image.min.y) / image.height().max(1.0)).clamp(0.0, 1.0);
    let x = (rel_x * fw as f32) as u32;
    let y = (rel_y * fh as f32) as u32;
    (x.min(fw.saturating_sub(1)), y.min(fh.saturating_sub(1)))
}

/// Wheel delta in ticks for the phone (positive = scroll left / up).
/// egui's delta is the content movement: positive x = content moves right,
/// i.e. the user scrolls left; positive y = content moves down (scroll up).
pub fn wheel_ticks(unit: MouseWheelUnit, dx: f32, dy: f32) -> (f32, f32) {
    let scale = match unit {
        MouseWheelUnit::Point => 1.0 / POINTS_PER_TICK,
        MouseWheelUnit::Line => 1.0,
        MouseWheelUnit::Page => 3.0,
    };
    (
        (dx * scale).clamp(-16.0, 16.0),
        (dy * scale).clamp(-16.0, 16.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    #[test]
    fn positions_map_to_frame_pixels() {
        // 1080 × 2400 frame drawn at half size from (100, 50).
        let image = Rect::from_min_size(pos2(100.0, 50.0), egui::vec2(540.0, 1200.0));
        let frame = (1080, 2400);
        assert_eq!(to_frame(pos2(100.0, 50.0), image, frame), (0, 0));
        assert_eq!(to_frame(pos2(370.0, 650.0), image, frame), (540, 1200));
        assert_eq!(
            to_frame(pos2(640.0, 1250.0), image, frame),
            (1079, 2399),
            "clamped"
        );
        assert_eq!(to_frame(pos2(0.0, 0.0), image, frame), (0, 0), "clamped");
    }

    #[test]
    fn wheel_units() {
        assert_eq!(wheel_ticks(MouseWheelUnit::Line, 0.0, 1.0), (0.0, 1.0));
        assert_eq!(wheel_ticks(MouseWheelUnit::Point, 0.0, -100.0), (0.0, -2.0));
        assert_eq!(
            wheel_ticks(MouseWheelUnit::Line, 0.0, 99.0).1,
            16.0,
            "clamped"
        );
    }

    #[test]
    fn keys_vs_text() {
        let none = Modifiers::default();
        let ctrl = Modifiers {
            ctrl: true,
            command: true,
            ..Modifiers::default()
        };
        let alt_gr = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::default()
        };
        assert!(matches!(
            key_input(Key::Enter, true, false, none),
            Some(PhoneInput::Key {
                key: PhoneKey::Enter,
                down: true,
                ..
            })
        ));
        assert_eq!(
            key_input(Key::A, true, false, none),
            None,
            "letters go as text"
        );
        assert!(matches!(
            key_input(Key::A, true, false, ctrl),
            Some(PhoneInput::Key {
                key: PhoneKey::Letter('a'),
                ctrl: true,
                ..
            })
        ));
        assert_eq!(
            key_input(Key::E, true, false, alt_gr),
            None,
            "AltGr+E is text (€)"
        );
        assert_eq!(
            key_input(Key::Num1, true, false, none),
            None,
            "digits go as text"
        );
    }
}
