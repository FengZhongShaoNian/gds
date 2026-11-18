use std::{
    cell::Cell,
    ffi::{OsStr, OsString},
    path::Path,
    rc::Rc,
};

use zgui::prelude::*;
use zgui::reactive::{RenderEffect, RwSignal, Signal, on_cleanup_local};
use zgui::view::install_stylesheet;
use zgui::vocab::{ImeEvent, Key, NamedKey, Role};
use zgui::{css, view};
use zgui_ui::{
    ButtonProps, ButtonVariant, DialogFooterProps, DialogHeaderProps, DialogTitleProps, InputProps,
};

use super::ui::Browser;
use crate::{features::file_operations::names, shared::i18n};

const NAME_FORM_SHEET: &str = "gds-files-name-form";
const NAME_FORM_CSS: &str = css!(
    r#"
    .gds-files-name-form {
        display: flex;
        flex-direction: column;
        gap: 12px;
        min-width: 340px;
        max-width: 440px;
    }

    .gds-files-name-form__message {
        margin: 0;
        color: var(--gds-muted-foreground);
        font-size: 13px;
    }

    .gds-files-name-form__error {
        margin: 0;
        color: var(--gds-error);
        font-size: 12px;
    }
    "#
);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NamePurpose {
    NewFolder,
    Rename,
    Conflict,
    Generic,
}

#[derive(Clone)]
pub(super) struct NameForm {
    purpose: NamePurpose,
    title: String,
    initial: OsString,
    display: String,
    non_utf8: bool,
    selection_end: usize,
}

impl NameForm {
    pub(super) fn new(purpose: NamePurpose, title: String, suggestion: OsString) -> Self {
        let non_utf8 = suggestion.to_str().is_none();
        let display = suggestion.to_string_lossy().into_owned();
        let selection_end = initial_selection_end(purpose, &display);
        Self {
            purpose,
            title,
            initial: suggestion,
            display,
            non_utf8,
            selection_end,
        }
    }

    fn checked_name(&self, value: &str) -> Result<OsString, &'static str> {
        if self.non_utf8 && value == self.display {
            names::validate(&self.initial).map_err(|error| error.label_id())?;
            return Ok(self.initial.clone());
        }
        names::validate(OsStr::new(value)).map_err(|error| error.label_id())?;
        Ok(OsString::from(value))
    }

    fn message_key(&self) -> Option<&'static str> {
        match self.purpose {
            NamePurpose::NewFolder => Some("new-folder-message"),
            NamePurpose::Rename => Some("rename-message"),
            NamePurpose::Conflict => Some("transfer-conflict-message"),
            NamePurpose::Generic => None,
        }
    }

    fn placeholder_key(&self) -> Option<&'static str> {
        match self.purpose {
            NamePurpose::NewFolder => Some("new-folder-placeholder"),
            NamePurpose::Rename => Some("rename-placeholder"),
            NamePurpose::Conflict => Some("transfer-conflict-placeholder"),
            NamePurpose::Generic => None,
        }
    }

    fn confirm_key(&self) -> &'static str {
        match self.purpose {
            NamePurpose::NewFolder => "new-folder-confirm",
            NamePurpose::Rename => "rename-confirm",
            NamePurpose::Conflict | NamePurpose::Generic => "dialog-confirm",
        }
    }

    pub(super) fn view(
        self,
        browser: &Browser,
        generation: u64,
        submit: Rc<dyn Fn(OsString)>,
    ) -> AnyView {
        if zgui::view::current_host().is_some() {
            install_stylesheet(NAME_FORM_SHEET, NAME_FORM_CSS);
        }

        let value = RwSignal::new_local(self.display.clone());
        let error = RwSignal::new_local(Option::<String>::None);
        let composing = Rc::new(Cell::new(false));
        let settled = Rc::new(Cell::new(false));
        let input_node = NodeRef::new();

        let initial_text = self.display.clone();
        let selection_end = self.selection_end;
        let focus_effect = RenderEffect::new(move |mounted_before: Option<bool>| {
            let mounted = input_node.get().is_some();
            if mounted && mounted_before != Some(true) {
                input_node.focus();
                input_node.set_value(&initial_text);
                input_node.set_selection(0..selection_end);
            }
            mounted
        });
        on_cleanup_local(move || drop(focus_effect));

        let clear_error_effect = RenderEffect::new(move |prev: Option<String>| {
            let current = value.get();
            if prev.as_deref() != Some(current.as_str()) && prev.is_some() {
                error.set(None);
            }
            current
        });
        on_cleanup_local(move || drop(clear_error_effect));

        let title = self.title.clone();
        let message = self.message_key().map(i18n::text);
        let placeholder = self.placeholder_key().map(i18n::text);
        let confirm_label = i18n::text(self.confirm_key());
        let cancel_label = i18n::text("dialog-cancel");
        let invalid = Signal::derive_local(move || error.with(Option::is_some));

        let form = Rc::new(self);
        let browser_submit = browser.clone();
        let submit_form = form.clone();
        let submit_settled = Rc::clone(&settled);
        let submit_action = Rc::new(move || {
            if submit_settled.get()
                || browser_submit.0.prompt_generation.get_untracked() != generation
                || matches!(
                    browser_submit.0.prompt.get_untracked(),
                    super::model::Prompt::Closed
                )
            {
                return;
            }
            let current = value.get_untracked();
            match submit_form.checked_name(&current) {
                Ok(validated) => {
                    submit_settled.set(true);
                    error.set(None);
                    // Close the active prompt before running `submit` so chained
                    // dialogs (for example, sequential conflict resolution) are not
                    // immediately overwritten with `Prompt::Closed`.
                    browser_submit.0.prompt.set(super::model::Prompt::Closed);
                    submit(validated);
                }
                Err(label_id) => {
                    error.set(Some(i18n::text_untracked(label_id)));
                    input_node.focus();
                }
            }
        });

        let browser_cancel = browser.clone();
        let cancel_settled = settled;
        let cancel_action = Rc::new(move || {
            if cancel_settled.get()
                || browser_cancel.0.prompt_generation.get_untracked() != generation
                || matches!(
                    browser_cancel.0.prompt.get_untracked(),
                    super::model::Prompt::Closed
                )
            {
                return;
            }
            cancel_settled.set(true);
            browser_cancel.cancel_prompt();
        });

        let key_submit = submit_action.clone();
        let key_cancel = cancel_action.clone();
        let key_composing = composing.clone();
        let start_composing = composing.clone();
        let preedit_composing = composing.clone();
        let commit_composing = composing.clone();
        let end_composing = composing.clone();
        let blur_composing = composing;

        let dialog_cancel = cancel_action.clone();
        let dialog_composing = blur_composing.clone();

        AnyView::new(view! {
            column(
                class = "gds-files-name-form",
                on:key_down = move |event| {
                    if !dialog_composing.get() && event.key == Key::Named(NamedKey::Escape) {
                        event.prevent_default();
                        event.stop_propagation();
                        dialog_cancel();
                    }
                },
            ) {
                DialogHeader {
                    DialogTitle { {title} }
                }
                {message.map(|text| view! {
                    text(class = "gds-files-name-form__message") { {text} }
                })}
                Input(
                    value = value,
                    placeholder = placeholder,
                    invalid = invalid,
                    node_ref = input_node,
                    on:ime_start = move |_| start_composing.set(true),
                    on:ime_preedit = move |event| {
                        let active = match &**event {
                            ImeEvent::Preedit { text, .. } => !text.is_empty(),
                            _ => event.is_composing(),
                        };
                        preedit_composing.set(active);
                    },
                    on:ime_commit = move |_| commit_composing.set(false),
                    on:ime_end = move |_| end_composing.set(false),
                    on:focus_out = move |_| blur_composing.set(false),
                    on:key_down = move |event| {
                        if key_composing.get() {
                            return;
                        }
                        if event.key == Key::Named(NamedKey::Enter)
                            && !event.modifiers.control()
                            && !event.modifiers.alt()
                            && !event.modifiers.meta()
                        {
                            event.prevent_default();
                            event.stop_propagation();
                            key_submit();
                        } else if event.key == Key::Named(NamedKey::Escape) {
                            event.prevent_default();
                            event.stop_propagation();
                            key_cancel();
                        }
                    },
                ) {}
                {move || error.get().map(|text| view! {
                    text(
                        class = "gds-files-name-form__error",
                        a11y:role = Role::Alert,
                    ) {
                        {text}
                    }
                })}
                DialogFooter {
                    row(class = "gds-dialog-actions") {
                        Button(
                            variant = ButtonVariant::Outline,
                            on:click = move |_| cancel_action(),
                        ) {
                            {cancel_label.clone()}
                        }
                        Button(
                            variant = ButtonVariant::Default,
                            on:click = move |_| submit_action(),
                        ) {
                            {confirm_label.clone()}
                        }
                    }
                }
            }
        })
    }
}

fn initial_selection_end(purpose: NamePurpose, display: &str) -> usize {
    if purpose == NamePurpose::Rename
        && let Some(stem) = Path::new(display)
            .file_stem()
            .and_then(|stem| stem.to_str())
        && !stem.is_empty()
        && display.starts_with(stem)
        && display.as_bytes().get(stem.len()) == Some(&b'.')
    {
        return stem.len();
    }
    display.len()
}
