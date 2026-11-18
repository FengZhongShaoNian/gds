//! Expandable disclosure chevron and collapsible section.

#[cfg(test)]
mod tests;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};
use zgui_ui::{CollapsibleContentProps, CollapsibleProps, CollapsibleTriggerProps};
use zgui_ui_primitives::Binding;

use crate::ui::icon::{IconName, IconProps};

/// Animated disclosure chevron (`>` rotated 90deg when open).
#[component]
pub fn DisclosureIcon(
    #[prop(into)] open: Signal<bool, LocalStorage>,
    #[prop(default = crate::appearance::tokens::ICON_SIZE)] size: f32,
    #[prop(into, optional)] class: Classes,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        box(
            class = "gds-disclosure-icon",
            attr:data-state = move || Some(if open.get() { "open" } else { "closed" }.to_owned()),
            class = class
        ) {
            Icon(name = IconName::ChevronRight, size = size)
        }
    }
}

/// Collapsible disclosure container built over `zgui_ui::Collapsible`.
#[component]
pub fn Disclosure(
    #[prop(into, optional)] open: Binding<bool>,
    #[prop(default = false)] default_open: bool,
    header: Children,
    content: Children,
) -> impl IntoView {
    view! {
        Collapsible(open = open, default_open = default_open) {
            CollapsibleTrigger {{header.into_view_once()}}
            CollapsibleContent {{content.into_view_once()}}
        }
    }
}
