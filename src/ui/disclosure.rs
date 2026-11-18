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
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        box(
            class = "gds-disclosure-icon",
            attr:data-state = move || Some(if open.get() { "open" } else { "closed" }.to_owned()),
            a11y:hidden = true,
            class = class,
            {..attrs}
        ) {
            Icon(name = IconName::ChevronRight, size = size)
        }
    }
}

/// Collapsible disclosure container built over `zgui_ui::Collapsible`.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn Disclosure(
    #[prop(into, optional)] open: Binding<bool>,
    #[prop(default = false)] default_open: bool,
    #[prop(optional)] on_open_change: Option<UnsyncCallback<bool>>,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
    #[prop(default = false)] show_icon: bool,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    header: Children,
    content: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();

    view! {
        Collapsible(
            open = open,
            default_open = default_open,
            on_open_change = on_open_change,
            disabled = disabled,
            node_ref = node_ref,
            class = "gds-disclosure",
            class = class,
            {..attrs}
        ) {
            CollapsibleTrigger(class = "gds-disclosure__trigger") {
                {show_icon.then(|| {
                    let context = zgui_ui::CollapsibleContext::current();
                    let is_open = Signal::derive_local(move || {
                        context.is_some_and(zgui_ui::CollapsibleContext::is_open)
                    });
                    view! { DisclosureIcon(open = is_open) }.into_view()
                })}
                {header.into_view_once()}
            }
            CollapsibleContent(class = "gds-disclosure__content") {{content.into_view_once()}}
        }
    }
}
