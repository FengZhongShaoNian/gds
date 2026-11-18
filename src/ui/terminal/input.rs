//! Keystroke and mouse-wheel translation into PTY byte sequences.

use zgui::vocab::{Key, Modifiers};

#[allow(dead_code)] // Seed skeleton for Wave 2 ui-terminal-tools.
#[must_use]
pub fn encode_key(key: &Key, modifiers: Modifiers) -> Option<Vec<u8>> {
    let _ = (key, modifiers);
    None
}
