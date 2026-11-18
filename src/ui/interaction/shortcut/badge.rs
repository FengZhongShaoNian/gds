use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui::{KbdGroupProps, KbdProps};

use super::Shortcut;

/// Visual keycap cluster displaying a [`Shortcut`].
#[component]
pub fn ShortcutBadge(shortcut: Shortcut, #[prop(into, optional)] class: Classes) -> impl IntoView {
    let tokens = shortcut.display_tokens();
    view! {
        KbdGroup(class = class) {
            for token in move || tokens.clone(), key = |t| t.clone() {
                Kbd {{token}}
            }
        }
    }
}
