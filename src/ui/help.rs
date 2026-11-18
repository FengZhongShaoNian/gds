//! Read-only help content and shortcut reference components.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

use super::{Shortcut, ShortcutBadgeProps};

/// Semantic trigger token for opening application help.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenHelp;

/// Scrollable keyboard-shortcut and reference page container.
#[component]
pub fn HelpPage(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        scroll(class = "gds-help-page", class = class, {..attrs}) {
            column(class = "gds-help-page__body") {
                {children.into_view_once()}
            }
        }
    }
}

/// Group of related shortcuts inside a [`HelpPage`].
#[component]
pub fn ShortcutSection(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        column(class = "gds-shortcut-section", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}

/// Section heading inside a [`ShortcutSection`].
#[component]
pub fn ShortcutHeading(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        text(class = "gds-shortcut-heading", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}

/// Single labelled keyboard shortcut row (or range of two shortcuts).
#[component]
pub fn ShortcutRow(
    #[prop(into)] label: Signal<String, LocalStorage>,
    shortcut: Shortcut,
    #[prop(optional)] range_end: Option<Shortcut>,
) -> impl IntoView {
    view! {
        row(class = "gds-shortcut-row") {
            text(class = "gds-shortcut-row__label") {{move || label.get()}}
            row(class = "gds-shortcut-row__keys") {
                ShortcutBadge(shortcut = shortcut) {}
                {range_end.map(|end| view! {
                    row {
                        text {"–"}
                        ShortcutBadge(shortcut = end)
                    }
                })}
            }
        }
    }
}
