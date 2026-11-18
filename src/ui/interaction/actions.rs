//! Shared command identities and keystroke defaults independent of GPUI.

use super::shortcut::Shortcut;
use zgui::vocab::{Key, Modifiers, NamedKey};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cancel;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FocusNotifications;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CloseNotification;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CloseAllNotifications;

/// Core semantic UI commands shared across windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    Cancel,
    FocusNotifications,
    CloseNotification,
    CloseAllNotifications,
}

impl Command {
    #[must_use]
    pub fn default_stroke(self) -> Shortcut {
        match self {
            Self::Cancel => Shortcut::new(Key::Named(NamedKey::Escape), Modifiers::NONE),
            Self::FocusNotifications => Shortcut::new(Key::character("n"), Modifiers::ALT),
            Self::CloseNotification => Shortcut::new(Key::character("c"), Modifiers::ALT),
            Self::CloseAllNotifications => {
                Shortcut::new(Key::character("c"), Modifiers::ALT | Modifiers::SHIFT)
            }
        }
    }
}
