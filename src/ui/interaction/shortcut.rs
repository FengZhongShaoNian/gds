//! Effective keybindings and their shared badge and tooltip presentation.

mod badge;
mod hint;
mod keycap;

pub use badge::{ShortcutBadge, ShortcutBadgeProps};
pub use hint::{ShortcutHint, ShortcutHintProps, command_tooltip, shortcut_tooltip};

use zgui::vocab::{Key, Modifiers, NamedKey};

use crate::ui::actions::Command;

/// Held modifier hint state used by keytip/shortcut chrome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeldModifier {
    #[default]
    None,
    Alt,
    Control,
    Shift,
    Meta,
}

/// Platform-aware keyboard shortcut binding.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl Shortcut {
    #[must_use]
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    #[must_use]
    pub fn command(command: Command) -> Self {
        command.default_stroke()
    }

    #[must_use]
    pub fn parse(spec: &str) -> Option<Self> {
        let mut modifiers = Modifiers::NONE;
        let mut key: Option<Key> = None;
        for part in spec.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
                "alt" | "option" => modifiers |= Modifiers::ALT,
                "shift" => modifiers |= Modifiers::SHIFT,
                "super" | "meta" | "cmd" | "win" => modifiers |= Modifiers::META,
                "primary" => modifiers |= Modifiers::PRIMARY,
                "esc" | "escape" => key = Some(Key::Named(NamedKey::Escape)),
                "enter" | "return" => key = Some(Key::Named(NamedKey::Enter)),
                "tab" => key = Some(Key::Named(NamedKey::Tab)),
                "space" => key = Some(Key::character(" ")),
                other => {
                    if let Ok(named) = other.parse::<NamedKey>() {
                        key = Some(Key::Named(named));
                    } else if let Ok(pascal) = part.parse::<NamedKey>() {
                        key = Some(Key::Named(pascal));
                    } else {
                        key = Some(Key::character(part.to_owned()));
                    }
                }
            }
        }
        key.map(|key| Self { key, modifiers })
    }

    #[must_use]
    pub fn matches(&self, key: &Key, modifiers: Modifiers) -> bool {
        let held = modifiers.bits() & Modifiers::ALL.bits();
        let wanted = self.modifiers.bits() & Modifiers::ALL.bits();
        if held != wanted {
            return false;
        }
        match (&self.key, key) {
            (Key::Character(a), Key::Character(b)) => a.as_str().eq_ignore_ascii_case(b.as_str()),
            _ => &self.key == key,
        }
    }

    #[must_use]
    pub fn display_tokens(&self) -> Vec<String> {
        let mut tokens = Vec::new();
        if self.modifiers.control() {
            tokens.push("Ctrl".to_owned());
        }
        if self.modifiers.alt() {
            tokens.push("Alt".to_owned());
        }
        if self.modifiers.shift() {
            tokens.push("Shift".to_owned());
        }
        if self.modifiers.meta() {
            tokens.push(if cfg!(target_os = "macos") {
                "Cmd".to_owned()
            } else {
                "Super".to_owned()
            });
        }
        tokens.push(keycap::display_key(&self.key));
        tokens
    }

    #[must_use]
    pub fn label(&self) -> String {
        self.display_tokens().join("+")
    }

    #[must_use]
    pub fn hint(&self, label: &str) -> String {
        format!("{label} · {}", self.label())
    }
}

impl From<Command> for Shortcut {
    fn from(command: Command) -> Self {
        Self::command(command)
    }
}
