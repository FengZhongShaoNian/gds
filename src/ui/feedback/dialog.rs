//! Native modal dialog footer actions, decisions, and prompt hosts.

mod anchor;
mod command;
mod decision;
mod footer;

pub use anchor::DialogAnchor;
pub use command::DialogCommand;
pub use decision::DialogDecision;
pub use footer::DialogFooter;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui::{ButtonProps, ButtonVariant};

/// Standard Confirm + Cancel button cluster used in modal dialog footers.
#[component]
pub fn DialogActions(
    #[prop(into)] confirm_label: Signal<String, LocalStorage>,
    #[prop(into, optional)] cancel_label: Option<String>,
    #[prop(default = false)] destructive: bool,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
    on_decision: UnsyncCallback<DialogDecision>,
) -> impl IntoView {
    let confirm_variant = if destructive {
        ButtonVariant::Destructive
    } else {
        ButtonVariant::Default
    };
    view! {
        row(class = "gds-dialog-actions") {
            {cancel_label.map(|label| {
                let on_cancel = on_decision;
                view! {
                    Button(
                        variant = ButtonVariant::Outline,
                        disabled = disabled,
                        on:click = move |_| on_cancel.run(DialogDecision::Cancel)
                    ) {{label}}
                }
                .into_view()
            })}
            Button(
                variant = confirm_variant,
                disabled = disabled,
                on:click = move |_| on_decision.run(DialogDecision::Confirm)
            ) {{move || confirm_label.get()}}
        }
    }
}
