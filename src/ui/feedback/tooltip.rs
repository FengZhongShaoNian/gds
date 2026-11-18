//! Shared GDS tooltip wrappers around `zgui-ui::Tooltip`.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};
use zgui_ui::{TooltipContentProps, TooltipProps, TooltipProviderProps, TooltipTriggerProps};

pub use crate::appearance::tokens::TOOLTIP_DELAY;

/// Window-root tooltip configuration provider with GDS hover delay.
#[component]
pub fn GdsTooltipProvider(children: Children) -> impl IntoView {
    view! {
        TooltipProvider(delay = TOOLTIP_DELAY) {
            {children.into_view_once()}
        }
    }
}

/// Plain localized text content for a GDS tooltip bubble.
#[component]
pub fn TooltipText(#[prop(into)] text: Signal<String, LocalStorage>) -> impl IntoView {
    view! {
        text(class = "gds-tooltip-text") {{move || text.get()}}
    }
}

/// Convenience helper pairing a trigger child with plain tooltip text.
#[component]
pub fn HintTooltip(
    #[prop(into)] text: Signal<String, LocalStorage>,
    children: Children,
) -> impl IntoView {
    view! {
        Tooltip {
            TooltipTrigger {{children.into_view_once()}}
            TooltipContent {
                TooltipText(text = text)
            }
        }
    }
}

/// Build plain tooltip body content matching legacy `tooltip::view(text)` callers.
#[must_use]
pub fn view(text: impl Into<String>) -> AnyView {
    let text = text.into();
    AnyView::new(view! { TooltipText(text = text) })
}
