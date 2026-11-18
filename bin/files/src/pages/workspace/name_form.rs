#![allow(dead_code)]

use std::{ffi::OsString, rc::Rc};

use gds::i18n;
use zgui::prelude::*;
use zgui::view;
use zgui_ui::{
    ButtonProps, ButtonVariant, DialogFooterProps, DialogHeaderProps, DialogTitleProps, InputProps,
};
use zgui_ui_primitives::Binding;

use super::ui::Browser;

#[derive(Clone, Copy)]
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
    suggestion: OsString,
    display: String,
}

impl NameForm {
    pub(super) fn new(purpose: NamePurpose, title: String, suggestion: OsString) -> Self {
        let display = suggestion.to_string_lossy().into_owned();
        Self {
            purpose,
            title,
            suggestion,
            display,
        }
    }

    fn checked_name(&self, value: &str) -> Result<OsString, &'static str> {
        // The suggestion can contain non-UTF-8 bytes that the native field displays lossily.
        if value == self.display {
            Ok(self.suggestion.clone())
        } else if value.trim().is_empty() {
            Err("name-empty")
        } else {
            Ok(OsString::from(value))
        }
    }

    pub(super) fn view(self, browser: &Browser, submit: Rc<dyn Fn(OsString)>) -> AnyView {
        let value = RwSignal::new_local(self.display.clone());
        let error = RwSignal::new_local(None::<&'static str>);
        let cancel = browser.clone();
        let close = browser.clone();
        let form = self.clone();
        let title = self.title.clone();
        let _ = self.purpose;

        let form_error = move || {
            error
                .get()
                .map(|key| view! { text {{i18n::text(key)}} }.into_view())
        };

        AnyView::new(view! {
            column {
                DialogHeader {
                    DialogTitle {{title}}
                }
                Input(
                    value = Binding::controlled(value, move |next: String| {
                        error.set(None);
                        value.set(next);
                    }),
                ) {}
                {form_error}
                DialogFooter {
                    Button(
                        variant = ButtonVariant::Outline,
                        on:click = move |_| cancel.cancel_prompt(),
                    ) {{i18n::text("dialog-cancel")}}
                    Button(
                        variant = ButtonVariant::Default,
                        on:click = move |_| match form.checked_name(&value.get_untracked()) {
                            Ok(name) => {
                                submit(name);
                                close.cancel_prompt();
                            }
                            Err(key) => error.set(Some(key)),
                        },
                    ) {{i18n::text("dialog-confirm")}}
                }
            }
        })
    }
}
