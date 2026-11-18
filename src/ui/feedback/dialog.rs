//! Native modal dialog footer actions, decisions, and prompt hosts over `zgui-ui::Dialog`.

mod anchor;
mod command;
mod decision;
mod footer;

pub use anchor::DialogAnchor;
pub use command::DialogCommand;
pub use decision::DialogDecision;
pub use footer::{DialogFooter, DialogFooterProps};
pub use zgui_ui::{
    Dialog, DialogClose, DialogCloseProps, DialogContent, DialogContentProps, DialogDescription,
    DialogDescriptionProps, DialogHeader, DialogHeaderProps, DialogProps, DialogTitle,
    DialogTitleProps,
};

use std::cell::Cell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, css, view};
use zgui_ui::{ButtonProps, ButtonVariant};
use zgui_ui_primitives::Binding;

use crate::app::Gds;

const SHEET: &str = css!(
    r#"
    .gds-dialog-content {
        min-width: 340px;
        max-width: 520px;
    }

    .gds-dialog-actions {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        justify-content: flex-end;
        gap: 8px;
    }
    "#
);

fn install_dialog_stylesheet() {
    if zgui::view::current_host().is_some() {
        zgui::view::install_stylesheet("gds-ui-dialog", SHEET);
    }
}

fn handle_dialog_key_down(
    disabled: bool,
    on_decision: UnsyncCallback<DialogDecision>,
    ev: &mut EventCx<'_, zgui::view::events::KeyDown>,
) {
    if !disabled && DialogCommand::Confirm.matches_event(ev, ev.modifiers) {
        on_decision.run(DialogDecision::Confirm);
        ev.prevent_default();
        ev.stop_propagation();
    } else if DialogCommand::Cancel.matches_event(ev, ev.modifiers) {
        on_decision.run(DialogDecision::Cancel);
        ev.prevent_default();
        ev.stop_propagation();
    }
}

/// Standard Confirm + Cancel button cluster used in modal dialog footers with explicit
/// AT-SPI `Role::Button` semantics.
#[component]
pub fn DialogActions(
    #[prop(into)] confirm_label: Signal<String, LocalStorage>,
    #[prop(into, optional)] cancel_label: Option<String>,
    #[prop(default = false)] destructive: bool,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
    on_decision: UnsyncCallback<DialogDecision>,
) -> impl IntoView {
    install_dialog_stylesheet();
    let confirm_variant = if destructive {
        ButtonVariant::Destructive
    } else {
        ButtonVariant::Default
    };
    view! {
        row(class = "gds-dialog-actions") {
            {cancel_label.map(|label| {
                let on_cancel = on_decision;
                let a11y_label = label.clone();
                view! {
                    Button(
                        variant = ButtonVariant::Outline,
                        disabled = disabled,
                        a11y:role = Role::Button,
                        a11y:label = a11y_label,
                        on:click = move |_| on_cancel.run(DialogDecision::Cancel)
                    ) {{label}}
                }
                .into_view()
            })}
            Button(
                variant = confirm_variant,
                disabled = disabled,
                a11y:role = Role::Button,
                a11y:label = move || confirm_label.get(),
                on:click = move |_| on_decision.run(DialogDecision::Confirm)
            ) {{move || confirm_label.get()}}
        }
    }
}

/// Controlled confirmation modal built over [`zgui_ui::Dialog`] with focus trap, `Escape`
/// dismissal, outside scrim dismissal, mnemonic `Alt+A` / `Alt+C` actions, and focus restoration
/// to `anchor`.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn ConfirmDialog(
    #[prop(into, optional)] open: Binding<bool>,
    #[prop(into)] title: Signal<String, LocalStorage>,
    #[prop(into, optional)] description: Option<Signal<String, LocalStorage>>,
    #[prop(into, optional)] confirm_label: Option<Signal<String, LocalStorage>>,
    #[prop(into, optional)] cancel_label: Option<String>,
    #[prop(default = false)] destructive: bool,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
    #[prop(optional)] anchor: Option<DialogAnchor>,
    on_decision: UnsyncCallback<DialogDecision>,
) -> impl IntoView {
    install_dialog_stylesheet();

    let confirm_label = confirm_label.unwrap_or_else(|| {
        let fallback = Gds::try_current().map_or_else(
            || "Confirm".to_owned(),
            |gds| gds.i18n().text_untracked("dialog-confirm"),
        );
        Signal::stored_local(fallback)
    });
    let cancel_label = cancel_label.unwrap_or_else(|| {
        Gds::try_current().map_or_else(
            || "Cancel".to_owned(),
            |gds| gds.i18n().text_untracked("dialog-cancel"),
        )
    });
    let cancel_sig = Signal::stored_local(cancel_label);

    let settled = Rc::new(Cell::new(false));
    let finish = {
        let settled = Rc::clone(&settled);
        UnsyncCallback::new(move |decision: DialogDecision| {
            if settled.replace(true) {
                return;
            }
            open.write(false);
            on_decision.run(decision);
            if let Some(anchor) = anchor {
                anchor.restore();
            }
        })
    };

    let on_open_change = {
        let settled = Rc::clone(&settled);
        UnsyncCallback::new(move |next_open: bool| {
            open.write(next_open);
            if next_open {
                settled.set(false);
            } else if !settled.replace(true) {
                on_decision.run(DialogDecision::Dismiss);
                if let Some(anchor) = anchor {
                    anchor.restore();
                }
            }
        })
    };

    let on_confirm_key = finish;
    view! {
        Dialog(open = open, on_open_change = on_open_change) {
            DialogContent(
                class = "gds-dialog-content",
                dismiss_on_outside_press = true,
                dismiss_on_escape = true,
                on:key_down = move |ev: &mut EventCx<'_, zgui::view::events::KeyDown>| {
                    handle_dialog_key_down(disabled.get_untracked(), on_confirm_key, ev);
                }
            ) {
                DialogHeader {
                    DialogTitle {{move || title.get()}}
                    {description.map(|desc| {
                        view! {
                            DialogDescription {{move || desc.get()}}
                        }
                        .into_view()
                    })}
                }
                DialogFooter {
                    DialogActions(
                        confirm_label = confirm_label,
                        cancel_label = cancel_sig.get_untracked(),
                        destructive = destructive,
                        disabled = disabled,
                        on_decision = finish,
                    )
                }
            }
        }
    }
}

/// Convenience helper returning an [`AnyView`] confirmation dialog prompt controlled by `open`.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn confirm(
    open: impl Into<Binding<bool>>,
    title: impl Into<String>,
    description: Option<String>,
    confirm_label: impl Into<String>,
    cancel_label: Option<String>,
    destructive: bool,
    anchor: Option<DialogAnchor>,
    on_decision: UnsyncCallback<DialogDecision>,
) -> AnyView {
    let open = open.into();
    let title = title.into();
    let confirm_label = confirm_label.into();
    let description = description.map(Signal::stored_local);
    AnyView::new(view! {
        ConfirmDialog(
            open = open,
            title = title,
            description = description,
            confirm_label = confirm_label,
            cancel_label = cancel_label,
            destructive = destructive,
            anchor = anchor,
            on_decision = on_decision,
        )
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use zgui::prelude::*;
    use zgui::reactive::{LocalStorage, Mounted, RwSignal, UnsyncCallback, install};
    use zgui::vocab::{KeyEvent, Modifiers};

    use super::{ConfirmDialog, DialogCommand, DialogDecision};

    #[test]
    fn dialog_commands_and_confirm_host_wire_up_cleanly() {
        assert_eq!(DialogCommand::Confirm.label_id(), "dialog-confirm");
        assert!(DialogCommand::Confirm.matches_event(&KeyEvent::character("a"), Modifiers::ALT));
        assert!(DialogCommand::Cancel.matches_event(&KeyEvent::character("c"), Modifiers::ALT));
        assert!(DialogDecision::Confirm.is_confirmed());
        assert!(DialogDecision::Cancel.is_cancelled());
        assert!(DialogDecision::Dismiss.is_cancelled());

        install().ok();
        let scope = Mounted::new();
        scope.with(|| {
            let open = RwSignal::new_local(true);
            let decided = Rc::new(Cell::new(None));
            let decided_cb = Rc::clone(&decided);
            let _dialog = ConfirmDialog(
                open.into(),
                Signal::<String, LocalStorage>::stored_local("Delete?".to_owned()),
                None,
                None,
                None,
                true,
                Signal::<bool, LocalStorage>::stored_local(false),
                None,
                UnsyncCallback::new(move |decision| decided_cb.set(Some(decision))),
            );
            assert!(open.get_untracked());
        });
        scope.unmount();
    }
}
