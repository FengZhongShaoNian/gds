use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, css, view};
use zgui_ui::{TooltipContentProps, TooltipProps, TooltipTriggerProps};

use super::{Shortcut, ShortcutBadgeProps};
use crate::appearance::tokens::TOOLTIP_DELAY;
use crate::ui::actions::Command;

const SHEET: &str = css!(
    r#"
    .gds-shortcut-hint {
        display: flex;
        align-items: center;
        gap: 8px;
    }
    "#
);

/// Tooltip wrapper displaying an action label paired with its [`ShortcutBadge`].
#[component]
pub fn ShortcutHint(
    #[prop(into)] label: Signal<String, LocalStorage>,
    #[prop(optional)] shortcut: Option<Shortcut>,
    children: Children,
) -> impl IntoView {
    if zgui::view::current_host().is_some() {
        zgui::view::install_stylesheet("gds-ui-shortcut-hint", SHEET);
    }
    view! {
        Tooltip(delay = TOOLTIP_DELAY) {
            TooltipTrigger {{children.into_view_once()}}
            TooltipContent {
                row(class = "gds-shortcut-hint") {
                    text {{move || label.get()}}
                    {shortcut.as_ref().map(|key| view! { ShortcutBadge(shortcut = key.clone()) }.into_view())}
                }
            }
        }
    }
}

/// Format a localized label with a command's default shortcut.
#[must_use]
pub fn command_tooltip(label: &str, command: Command) -> String {
    command.default_stroke().hint(label)
}

/// Format a localized label with an explicit shortcut.
#[must_use]
pub fn shortcut_tooltip(label: &str, shortcut: &Shortcut) -> String {
    shortcut.hint(label)
}
