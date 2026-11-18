use zgui::prelude::*;
use zgui::{component, css, view};
use zgui_ui::{KbdGroupProps, KbdProps};

use super::Shortcut;

const SHEET: &str = css!(
    r#"
    .gds-shortcut-badge {
        display: inline-flex;
        align-items: center;
        gap: 3px;
    }
    "#
);

/// Visual keycap cluster displaying a [`Shortcut`].
#[component]
pub fn ShortcutBadge(shortcut: Shortcut, #[prop(into, optional)] class: Classes) -> impl IntoView {
    if zgui::view::current_host().is_some() {
        zgui::view::install_stylesheet("gds-ui-shortcut-badge", SHEET);
    }
    let tokens = shortcut.display_tokens();
    view! {
        KbdGroup(class = "gds-shortcut-badge", class = class) {
            for token in move || tokens.clone(), key = |token| token.clone() {
                Kbd {{token}}
            }
        }
    }
}
