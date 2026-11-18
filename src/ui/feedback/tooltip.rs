//! Shared GDS tooltip wrappers around `zgui-ui::Tooltip`.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, css, view};
use zgui_ui::{TooltipContentProps, TooltipProps, TooltipProviderProps, TooltipTriggerProps};

pub use crate::appearance::tokens::TOOLTIP_DELAY;

const SHEET: &str = css!(
    r#"
    .gds-tooltip-content {
        max-width: 320px;
        max-height: 220px;
        overflow-y: auto;
    }

    .gds-tooltip-text {
        overflow-wrap: anywhere;
        line-height: 1.35;
    }

    .gds-tooltip-details {
        display: flex;
        flex-direction: column;
        gap: 4px;
        max-width: 320px;
        max-height: 220px;
        overflow-y: auto;
    }

    .gds-tooltip-details__item {
        font-size: 12px;
        color: var(--gds-muted-foreground);
        overflow-wrap: anywhere;
    }
    "#
);

/// Inserts soft break opportunities after path separators and punctuation so long unspaced
/// identifiers wrap within tooltip bounds cleanly.
#[must_use]
pub fn wrap_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        out.push(ch);
        if matches!(ch, '/' | '\\' | ':' | '-' | '_') {
            out.push('\u{200B}');
        }
    }
    out
}

fn install_tooltip_stylesheet() {
    if zgui::view::current_host().is_some() {
        zgui::view::install_stylesheet("gds-ui-tooltip", SHEET);
    }
}

/// Window-root tooltip configuration provider with GDS 400ms hover delay.
#[component]
pub fn GdsTooltipProvider(children: Children) -> impl IntoView {
    install_tooltip_stylesheet();
    view! {
        TooltipProvider(delay = TOOLTIP_DELAY) {
            {children.into_view_once()}
        }
    }
}

/// Plain localized text content for a GDS tooltip bubble.
#[component]
pub fn TooltipText(#[prop(into)] text: Signal<String, LocalStorage>) -> impl IntoView {
    install_tooltip_stylesheet();
    view! {
        text(class = "gds-tooltip-text") {{move || text.get()}}
    }
}

/// Convenience helper pairing a trigger child with plain tooltip text using the 400ms GDS delay.
#[component]
pub fn HintTooltip(
    #[prop(into)] text: Signal<String, LocalStorage>,
    children: Children,
) -> impl IntoView {
    install_tooltip_stylesheet();
    view! {
        Tooltip(delay = TOOLTIP_DELAY) {
            TooltipTrigger {{children.into_view_once()}}
            TooltipContent(class = "gds-tooltip-content") {
                TooltipText(text = text)
            }
        }
    }
}

/// Tooltip wrapper pairing a summary line with secondary detail lines.
#[component]
pub fn DetailsTooltip(
    #[prop(into)] summary: Signal<String, LocalStorage>,
    #[prop(into)] details: Signal<Vec<String>, LocalStorage>,
    children: Children,
) -> impl IntoView {
    install_tooltip_stylesheet();
    view! {
        Tooltip(delay = TOOLTIP_DELAY) {
            TooltipTrigger {{children.into_view_once()}}
            TooltipContent(class = "gds-tooltip-content") {
                column(class = "gds-tooltip-details") {
                    TooltipText(text = summary)
                    for line in move || details.get(), key = |line| line.clone() {
                        text(class = "gds-tooltip-details__item") {{line}}
                    }
                }
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{TOOLTIP_DELAY, wrap_tokens};

    #[test]
    fn tooltip_delay_and_token_wrapping_match_spec() {
        assert_eq!(TOOLTIP_DELAY, Duration::from_millis(400));
        assert_eq!(
            wrap_tokens("/home/very-long"),
            "/\u{200B}home/\u{200B}very-\u{200B}long"
        );
    }
}
