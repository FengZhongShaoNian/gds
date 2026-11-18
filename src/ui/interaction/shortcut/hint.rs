use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};
use zgui_ui::{TooltipContentProps, TooltipProps, TooltipTriggerProps};

use super::{Shortcut, ShortcutBadgeProps};
use crate::ui::actions::Command;

/// Tooltip wrapper displaying an action label paired with its [`ShortcutBadge`].
#[component]
pub fn ShortcutHint(
    #[prop(into)] label: Signal<String, LocalStorage>,
    #[prop(optional)] shortcut: Option<Shortcut>,
    children: Children,
) -> impl IntoView {
    view! {
        Tooltip {
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
