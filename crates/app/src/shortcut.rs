//! Keyboard shortcut to mark a cut: Ctrl+M by default, can be changed in the
//! settings. It always works, even while the phone has the keyboard (it is
//! taken before the keys are sent to the phone), so it must never be
//! something the user types: Ctrl or Alt is required, and combinations the
//! phone or Windows use are refused.

use eframe::egui::{Key, KeyboardShortcut, Modifiers};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// egui key name, e.g. "M", "5", "F2".
    pub key: String,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self {
            ctrl: true,
            alt: false,
            shift: false,
            key: "M".into(),
        }
    }
}

/// Why a combination cannot be the shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutProblem {
    /// A key alone (or with Shift): it would be typing for the phone.
    NeedsCtrlOrAlt,
    /// Ctrl+Alt is AltGr on many keyboards (€, @…): typing again.
    AltGr,
    /// Already used: copy/paste/undo on the phone, F11 full screen, Alt+F4…
    Reserved,
    /// Not a letter, a digit or a function key.
    UnsupportedKey,
    /// Already the shortcut of the other action.
    InUse,
}

impl Shortcut {
    /// Ctrl+R: save the instant replay.
    pub fn replay_default() -> Self {
        Self {
            key: "R".into(),
            ..Self::default()
        }
    }

    /// The combination pressed, if it can be the shortcut.
    pub fn from_keys(key: Key, modifiers: Modifiers) -> Result<Self, ShortcutProblem> {
        let name = key.name();
        let letter_or_digit = name.len() == 1 && name.chars().all(|c| c.is_ascii_alphanumeric());
        let function =
            name.starts_with('F') && name[1..].parse::<u8>().is_ok_and(|n| (1..=12).contains(&n));
        if !letter_or_digit && !function {
            return Err(ShortcutProblem::UnsupportedKey);
        }
        if key == Key::F11 || (modifiers.alt && key == Key::F4) {
            return Err(ShortcutProblem::Reserved);
        }
        if !modifiers.ctrl && !modifiers.alt {
            return Err(ShortcutProblem::NeedsCtrlOrAlt);
        }
        if modifiers.ctrl && modifiers.alt {
            return Err(ShortcutProblem::AltGr);
        }
        // Editing shortcuts sent to the phone.
        if modifiers.ctrl && matches!(key, Key::A | Key::C | Key::V | Key::X | Key::Y | Key::Z) {
            return Err(ShortcutProblem::Reserved);
        }
        Ok(Self {
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            shift: modifiers.shift,
            key: name.to_owned(),
        })
    }

    /// For egui; `None` if the saved key name is unknown.
    pub fn keyboard_shortcut(&self) -> Option<KeyboardShortcut> {
        let key = Key::from_name(&self.key)?;
        let modifiers = Modifiers {
            alt: self.alt,
            ctrl: self.ctrl,
            shift: self.shift,
            mac_cmd: false,
            command: self.ctrl,
        };
        Some(KeyboardShortcut::new(modifiers, key))
    }

    /// e.g. "Ctrl+M", "Alt+Shift+F2".
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push(crate::texts::shift_key());
        }
        parts.push(&self.key);
        parts.join("+")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(ctrl: bool, alt: bool, shift: bool) -> Modifiers {
        Modifiers {
            alt,
            ctrl,
            shift,
            mac_cmd: false,
            command: ctrl,
        }
    }

    #[test]
    fn default_is_ctrl_m() {
        let shortcut = Shortcut::default();
        assert_eq!(shortcut.label(), "Ctrl+M");
        let egui = shortcut.keyboard_shortcut().unwrap();
        assert_eq!(egui.logical_key, Key::M);
        assert!(egui.modifiers.ctrl);
    }

    #[test]
    fn plain_keys_are_refused() {
        let plain = |key| Shortcut::from_keys(key, mods(false, false, false));
        assert_eq!(plain(Key::M), Err(ShortcutProblem::NeedsCtrlOrAlt));
        assert_eq!(plain(Key::Num5), Err(ShortcutProblem::NeedsCtrlOrAlt));
        assert_eq!(
            Shortcut::from_keys(Key::M, mods(false, false, true)),
            Err(ShortcutProblem::NeedsCtrlOrAlt),
            "Shift+M is a capital M"
        );
    }

    #[test]
    fn conflicts_are_refused() {
        assert_eq!(
            Shortcut::from_keys(Key::V, mods(true, false, false)),
            Err(ShortcutProblem::Reserved),
            "paste on the phone"
        );
        assert_eq!(
            Shortcut::from_keys(Key::E, mods(true, true, false)),
            Err(ShortcutProblem::AltGr)
        );
        assert_eq!(
            Shortcut::from_keys(Key::F4, mods(false, true, false)),
            Err(ShortcutProblem::Reserved)
        );
        assert_eq!(
            Shortcut::from_keys(Key::F11, mods(true, false, false)),
            Err(ShortcutProblem::Reserved)
        );
        assert_eq!(
            Shortcut::from_keys(Key::Space, mods(true, false, false)),
            Err(ShortcutProblem::UnsupportedKey)
        );
    }

    #[test]
    fn good_combinations_are_kept() {
        let ok = Shortcut::from_keys(Key::K, mods(true, false, true)).unwrap();
        assert_eq!(ok.label(), format!("Ctrl+{}+K", crate::texts::shift_key()));
        assert!(ok.keyboard_shortcut().is_some());
        assert!(Shortcut::from_keys(Key::Num1, mods(false, true, false)).is_ok());
        assert!(Shortcut::from_keys(Key::F2, mods(true, false, false)).is_ok());
    }
}
