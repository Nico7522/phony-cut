//! Control messages sent to the scrcpy **v5.0.1** server (mouse, keyboard,
//! navigation buttons), and the translation from PhonyCut's input events.
//!
//! Verified against scrcpy v5.0.1 `app/src/control_msg.{h,c}`,
//! `app/tests/test_control_msg_serialize.c` (the tests below reuse its byte
//! vectors), `mouse_sdk.c`, `keyboard_sdk.c` and `android/{input,keycodes}.h`.

use crate::api::{PhoneButton, PhoneInput, PhoneKey, TouchPhase};

/// Pointer id used by scrcpy for the mouse (`SC_POINTER_ID_MOUSE` = -1).
pub const POINTER_ID_MOUSE: u64 = u64::MAX;
/// Longest text sent in one message (bytes, `SC_CONTROL_MSG_INJECT_TEXT_MAX_LENGTH`).
pub const INJECT_TEXT_MAX_LENGTH: usize = 300;

// Message types (enum sc_control_msg_type).
const TYPE_INJECT_KEYCODE: u8 = 0;
const TYPE_INJECT_TEXT: u8 = 1;
const TYPE_INJECT_TOUCH_EVENT: u8 = 2;
const TYPE_INJECT_SCROLL_EVENT: u8 = 3;
const TYPE_BACK_OR_SCREEN_ON: u8 = 4;

// android/input.h
const KEY_ACTION_DOWN: u8 = 0;
const KEY_ACTION_UP: u8 = 1;
const MOTION_ACTION_DOWN: u8 = 0;
const MOTION_ACTION_UP: u8 = 1;
const MOTION_ACTION_MOVE: u8 = 2;
const BUTTON_PRIMARY: u32 = 1;
const META_SHIFT_ON: u32 = 0x01;
const META_ALT_ON: u32 = 0x02;
const META_ALT_LEFT_ON: u32 = 0x10;
const META_SHIFT_LEFT_ON: u32 = 0x40;
const META_CTRL_ON: u32 = 0x1000;
const META_CTRL_LEFT_ON: u32 = 0x2000;

// android/keycodes.h
mod keycode {
    pub const HOME: u32 = 3;
    pub const DPAD_UP: u32 = 19;
    pub const DPAD_DOWN: u32 = 20;
    pub const DPAD_LEFT: u32 = 21;
    pub const DPAD_RIGHT: u32 = 22;
    pub const VOLUME_UP: u32 = 24;
    pub const VOLUME_DOWN: u32 = 25;
    pub const POWER: u32 = 26;
    pub const A: u32 = 29;
    pub const TAB: u32 = 61;
    pub const SPACE: u32 = 62;
    pub const ENTER: u32 = 66;
    pub const DEL: u32 = 67;
    pub const PAGE_UP: u32 = 92;
    pub const PAGE_DOWN: u32 = 93;
    pub const ESCAPE: u32 = 111;
    pub const FORWARD_DEL: u32 = 112;
    pub const MOVE_HOME: u32 = 122;
    pub const MOVE_END: u32 = 123;
    pub const APP_SWITCH: u32 = 187;
}

/// A point on the video frame, with the frame size it refers to (the server
/// ignores it if the size no longer matches, e.g. right after a rotation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlMessage {
    InjectKeycode {
        down: bool,
        keycode: u32,
        repeat: u32,
        metastate: u32,
    },
    InjectText(String),
    InjectTouch {
        action: u8,
        pointer_id: u64,
        position: Position,
        /// 0.0 ..= 1.0
        pressure: f32,
        action_button: u32,
        buttons: u32,
    },
    InjectScroll {
        position: Position,
        /// Wheel ticks, -16 ..= 16 (positive = left / up).
        hscroll: f32,
        vscroll: f32,
        buttons: u32,
    },
    /// BACK, or turns the screen on if it is off.
    BackOrScreenOn {
        down: bool,
    },
}

impl ControlMessage {
    /// Wire format (big-endian), as `sc_control_msg_serialize`.
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        match self {
            Self::InjectKeycode {
                down,
                keycode,
                repeat,
                metastate,
            } => {
                out.push(TYPE_INJECT_KEYCODE);
                out.push(if *down {
                    KEY_ACTION_DOWN
                } else {
                    KEY_ACTION_UP
                });
                out.extend(keycode.to_be_bytes());
                out.extend(repeat.to_be_bytes());
                out.extend(metastate.to_be_bytes());
            }
            Self::InjectText(text) => {
                out.push(TYPE_INJECT_TEXT);
                let text = truncate_utf8(text, INJECT_TEXT_MAX_LENGTH);
                out.extend((text.len() as u32).to_be_bytes());
                out.extend(text.as_bytes());
            }
            Self::InjectTouch {
                action,
                pointer_id,
                position,
                pressure,
                action_button,
                buttons,
            } => {
                out.push(TYPE_INJECT_TOUCH_EVENT);
                out.push(*action);
                out.extend(pointer_id.to_be_bytes());
                write_position(&mut out, position);
                out.extend(float_to_u16fp(*pressure).to_be_bytes());
                out.extend(action_button.to_be_bytes());
                out.extend(buttons.to_be_bytes());
            }
            Self::InjectScroll {
                position,
                hscroll,
                vscroll,
                buttons,
            } => {
                out.push(TYPE_INJECT_SCROLL_EVENT);
                write_position(&mut out, position);
                // [-16, 16] normalized to [-1, 1], then signed 16-bit fixed point.
                out.extend(float_to_i16fp((hscroll / 16.0).clamp(-1.0, 1.0)).to_be_bytes());
                out.extend(float_to_i16fp((vscroll / 16.0).clamp(-1.0, 1.0)).to_be_bytes());
                out.extend(buttons.to_be_bytes());
            }
            Self::BackOrScreenOn { down } => {
                out.push(TYPE_BACK_OR_SCREEN_ON);
                out.push(if *down {
                    KEY_ACTION_DOWN
                } else {
                    KEY_ACTION_UP
                });
            }
        }
        out
    }
}

fn write_position(out: &mut Vec<u8>, p: &Position) {
    out.extend(p.x.to_be_bytes());
    out.extend(p.y.to_be_bytes());
    out.extend(p.width.to_be_bytes());
    out.extend(p.height.to_be_bytes());
}

/// `sc_float_to_u16fp`: [0, 1] → u16 (1.0 → 0xffff).
fn float_to_u16fp(f: f32) -> u16 {
    let u = (f.clamp(0.0, 1.0) * 65536.0) as u32;
    u.min(0xffff) as u16
}

/// `sc_float_to_i16fp`: [-1, 1] → i16 (1.0 → 0x7fff).
fn float_to_i16fp(f: f32) -> i16 {
    let i = (f.clamp(-1.0, 1.0) * 32768.0) as i32;
    i.clamp(-0x8000, 0x7fff) as i16
}

/// Longest prefix of `text` of at most `max` bytes, cut on a char boundary.
fn truncate_utf8(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Splits a long text into messages of at most [`INJECT_TEXT_MAX_LENGTH`] bytes.
fn text_messages(text: &str) -> Vec<ControlMessage> {
    let mut messages = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let chunk = truncate_utf8(rest, INJECT_TEXT_MAX_LENGTH);
        let chunk = if chunk.is_empty() {
            // A single char longer than the limit cannot happen in UTF-8
            // (≤ 4 bytes); guard anyway.
            &rest[..rest.chars().next().map_or(rest.len(), char::len_utf8)]
        } else {
            chunk
        };
        messages.push(ControlMessage::InjectText(chunk.to_owned()));
        rest = &rest[chunk.len()..];
    }
    messages
}

fn key_press(keycode: u32, metastate: u32) -> Vec<ControlMessage> {
    [true, false]
        .into_iter()
        .map(|down| ControlMessage::InjectKeycode {
            down,
            keycode,
            repeat: 0,
            metastate,
        })
        .collect()
}

fn position(x: u32, y: u32, width: u32, height: u32) -> Position {
    let clamp = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
    let size = |v: u32| u16::try_from(v).unwrap_or(u16::MAX);
    Position {
        x: clamp(x),
        y: clamp(y),
        width: size(width),
        height: size(height),
    }
}

/// Translates a PhonyCut input event into scrcpy control messages.
pub fn translate(input: &PhoneInput) -> Vec<ControlMessage> {
    match input {
        PhoneInput::Touch {
            phase,
            x,
            y,
            frame_width,
            frame_height,
        } => {
            let (action, pressure, buttons) = match phase {
                TouchPhase::Down => (MOTION_ACTION_DOWN, 1.0, BUTTON_PRIMARY),
                TouchPhase::Move => (MOTION_ACTION_MOVE, 1.0, BUTTON_PRIMARY),
                TouchPhase::Up => (MOTION_ACTION_UP, 0.0, 0),
            };
            let action_button = match phase {
                TouchPhase::Move => 0,
                TouchPhase::Down | TouchPhase::Up => BUTTON_PRIMARY,
            };
            vec![ControlMessage::InjectTouch {
                action,
                pointer_id: POINTER_ID_MOUSE,
                position: position(*x, *y, *frame_width, *frame_height),
                pressure,
                action_button,
                buttons,
            }]
        }
        PhoneInput::Scroll {
            x,
            y,
            frame_width,
            frame_height,
            horizontal,
            vertical,
        } => vec![ControlMessage::InjectScroll {
            position: position(*x, *y, *frame_width, *frame_height),
            hscroll: horizontal.clamp(-16.0, 16.0),
            vscroll: vertical.clamp(-16.0, 16.0),
            buttons: 0,
        }],
        PhoneInput::Text(text) => text_messages(text),
        PhoneInput::Key {
            key,
            down,
            repeat,
            shift,
            ctrl,
            alt,
        } => {
            let mut metastate = 0;
            if *shift {
                metastate |= META_SHIFT_ON | META_SHIFT_LEFT_ON;
            }
            if *ctrl {
                metastate |= META_CTRL_ON | META_CTRL_LEFT_ON;
            }
            if *alt {
                metastate |= META_ALT_ON | META_ALT_LEFT_ON;
            }
            vec![ControlMessage::InjectKeycode {
                down: *down,
                keycode: key_code(*key),
                repeat: *repeat,
                metastate,
            }]
        }
        PhoneInput::Button(button) => match button {
            PhoneButton::Back => vec![
                ControlMessage::BackOrScreenOn { down: true },
                ControlMessage::BackOrScreenOn { down: false },
            ],
            PhoneButton::Home => key_press(keycode::HOME, 0),
            PhoneButton::AppSwitch => key_press(keycode::APP_SWITCH, 0),
            PhoneButton::VolumeUp => key_press(keycode::VOLUME_UP, 0),
            PhoneButton::VolumeDown => key_press(keycode::VOLUME_DOWN, 0),
            PhoneButton::Power => key_press(keycode::POWER, 0),
        },
    }
}

fn key_code(key: PhoneKey) -> u32 {
    match key {
        PhoneKey::Enter => keycode::ENTER,
        PhoneKey::Backspace => keycode::DEL,
        PhoneKey::Delete => keycode::FORWARD_DEL,
        PhoneKey::Tab => keycode::TAB,
        PhoneKey::Escape => keycode::ESCAPE,
        PhoneKey::Up => keycode::DPAD_UP,
        PhoneKey::Down => keycode::DPAD_DOWN,
        PhoneKey::Left => keycode::DPAD_LEFT,
        PhoneKey::Right => keycode::DPAD_RIGHT,
        PhoneKey::Home => keycode::MOVE_HOME,
        PhoneKey::End => keycode::MOVE_END,
        PhoneKey::PageUp => keycode::PAGE_UP,
        PhoneKey::PageDown => keycode::PAGE_DOWN,
        PhoneKey::Space => keycode::SPACE,
        // AKEYCODE_A..=AKEYCODE_Z are consecutive.
        PhoneKey::Letter(c) => {
            let offset = u32::from(c.to_ascii_lowercase()).saturating_sub(u32::from('a'));
            keycode::A + offset.min(25)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Byte vectors from scrcpy v5.0.1 app/tests/test_control_msg_serialize.c.

    #[test]
    fn serialize_inject_keycode() {
        let msg = ControlMessage::InjectKeycode {
            down: false,
            keycode: keycode::ENTER,
            repeat: 5,
            metastate: META_SHIFT_ON | META_SHIFT_LEFT_ON,
        };
        assert_eq!(
            msg.serialize(),
            [0, 0x01, 0, 0, 0, 0x42, 0, 0, 0, 0x05, 0, 0, 0, 0x41]
        );
    }

    #[test]
    fn serialize_inject_text() {
        let msg = ControlMessage::InjectText("hello, world!".into());
        let mut expected = vec![1, 0, 0, 0, 0x0d];
        expected.extend(b"hello, world!");
        assert_eq!(msg.serialize(), expected);
    }

    #[test]
    fn serialize_inject_text_long_is_truncated() {
        let msg = ControlMessage::InjectText("a".repeat(INJECT_TEXT_MAX_LENGTH + 1));
        let bytes = msg.serialize();
        assert_eq!(bytes.len(), 5 + INJECT_TEXT_MAX_LENGTH);
        assert_eq!(&bytes[..5], &[1, 0, 0, 0x01, 0x2c]);
    }

    #[test]
    fn serialize_inject_touch_event() {
        let msg = ControlMessage::InjectTouch {
            action: MOTION_ACTION_DOWN,
            pointer_id: 0x1234_5678_8765_4321,
            position: Position {
                x: 100,
                y: 200,
                width: 1080,
                height: 1920,
            },
            pressure: 1.0,
            action_button: BUTTON_PRIMARY,
            buttons: BUTTON_PRIMARY,
        };
        assert_eq!(
            msg.serialize(),
            [
                2, 0x00, //
                0x12, 0x34, 0x56, 0x78, 0x87, 0x65, 0x43, 0x21, //
                0, 0, 0, 0x64, 0, 0, 0, 0xc8, //
                0x04, 0x38, 0x07, 0x80, //
                0xff, 0xff, //
                0, 0, 0, 0x01, //
                0, 0, 0, 0x01,
            ]
        );
    }

    #[test]
    fn serialize_inject_scroll_event() {
        let msg = ControlMessage::InjectScroll {
            position: Position {
                x: 260,
                y: 1026,
                width: 1080,
                height: 1920,
            },
            hscroll: 16.0,
            vscroll: -16.0,
            buttons: 1,
        };
        assert_eq!(
            msg.serialize(),
            [
                3, //
                0, 0, 0x01, 0x04, 0, 0, 0x04, 0x02, //
                0x04, 0x38, 0x07, 0x80, //
                0x7f, 0xff, //
                0x80, 0x00, //
                0, 0, 0, 0x01,
            ]
        );
    }

    #[test]
    fn serialize_back_or_screen_on() {
        assert_eq!(
            ControlMessage::BackOrScreenOn { down: false }.serialize(),
            [4, 0x01]
        );
    }

    #[test]
    fn truncation_respects_utf8() {
        let text = format!("{}é", "a".repeat(INJECT_TEXT_MAX_LENGTH - 1)); // é = 2 bytes
        assert_eq!(
            truncate_utf8(&text, INJECT_TEXT_MAX_LENGTH).len(),
            INJECT_TEXT_MAX_LENGTH - 1
        );
        let parts = text_messages(&text);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1], ControlMessage::InjectText("é".into()));
    }

    #[test]
    fn mouse_click_becomes_down_then_up() {
        let at = |phase| PhoneInput::Touch {
            phase,
            x: 10,
            y: 20,
            frame_width: 1080,
            frame_height: 2400,
        };
        let down = translate(&at(TouchPhase::Down));
        assert!(matches!(
            down[0],
            ControlMessage::InjectTouch {
                action: MOTION_ACTION_DOWN,
                pointer_id: POINTER_ID_MOUSE,
                buttons: BUTTON_PRIMARY,
                ..
            }
        ));
        let up = translate(&at(TouchPhase::Up));
        assert!(matches!(
            up[0],
            ControlMessage::InjectTouch {
                action: MOTION_ACTION_UP,
                buttons: 0,
                action_button: BUTTON_PRIMARY,
                ..
            }
        ));
    }

    #[test]
    fn buttons_and_keys() {
        assert_eq!(
            translate(&PhoneInput::Button(PhoneButton::Back)),
            vec![
                ControlMessage::BackOrScreenOn { down: true },
                ControlMessage::BackOrScreenOn { down: false }
            ]
        );
        let home = translate(&PhoneInput::Button(PhoneButton::Home));
        assert_eq!(home.len(), 2);
        assert!(matches!(
            home[0],
            ControlMessage::InjectKeycode {
                keycode: keycode::HOME,
                down: true,
                ..
            }
        ));
        let ctrl_a = translate(&PhoneInput::Key {
            key: PhoneKey::Letter('A'),
            down: true,
            repeat: 0,
            shift: false,
            ctrl: true,
            alt: false,
        });
        assert_eq!(
            ctrl_a,
            vec![ControlMessage::InjectKeycode {
                down: true,
                keycode: keycode::A,
                repeat: 0,
                metastate: META_CTRL_ON | META_CTRL_LEFT_ON
            }]
        );
        assert_eq!(key_code(PhoneKey::Letter('z')), 54);
    }
}
