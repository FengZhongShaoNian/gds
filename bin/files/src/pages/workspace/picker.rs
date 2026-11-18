//! Native chooser controls over the existing lossless picker protocol.

use std::{
    cell::Cell,
    ffi::OsString,
    path::{Path, PathBuf},
    rc::Rc,
};

use gds::service::desktop::protocol::chooser;
use zgui::prelude::*;
use zgui::reactive::{RenderEffect, Signal, UnsyncCallback, on_cleanup_local};
use zgui::view::install_stylesheet;
use zgui::vocab::{ImeEvent, Key, NamedKey};
use zgui::{css, view};
use zgui_ui::{
    ButtonProps, ButtonVariant, CheckboxProps, Checked, InputProps, SelectContentProps,
    SelectItemProps, SelectProps, SelectTriggerProps, SelectTriggerSize, SelectValueProps,
};
use zgui_ui_primitives::Binding;

use super::{model::Contents, ui::Browser};
use crate::{
    features::pick_files::{
        ActivatedPath, Options, SaveDecision, inspect_activated_path, prepare_save_targets,
    },
    shared::i18n,
};

const PICKER_SHEET: &str = "gds-files-picker-footer";
const PICKER_CSS: &str = css!(
    r#"
    .gds-files-picker-footer {
        display: flex;
        flex-direction: column;
        gap: 10px;
        padding: 10px 14px;
        border-top: 1px solid var(--gds-border);
        background: var(--gds-panel);
    }

    .gds-files-picker-footer__row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 10px;
        flex-wrap: wrap;
    }

    .gds-files-picker-footer__field {
        display: flex;
        align-items: center;
        gap: 8px;
        min-width: 0;
        flex: 1;
    }

    .gds-files-picker-footer__label {
        font-size: 13px;
        color: var(--gds-muted-foreground);
        white-space: nowrap;
    }

    .gds-files-picker-footer__controls {
        display: flex;
        align-items: center;
        gap: 10px;
        flex-wrap: wrap;
    }

    .gds-files-picker-footer__actions {
        display: flex;
        align-items: center;
        gap: 8px;
        margin-left: auto;
    }
    "#
);

impl Browser {
    pub(super) fn initialize_picker(&self) {
        let initial = self.read(|state| {
            state
                .picker
                .as_ref()
                .and_then(|picker| picker.save_name.as_ref())
                .map(|name| name.to_string_lossy().into_owned())
        });
        if let Some(name) = initial {
            self.0.save_name.set(name);
        }
    }

    pub(super) fn picker_footer(&self) -> AnyView {
        if self.read(|state| state.picker.is_none()) {
            return AnyView::new(view! { box() });
        }
        if zgui::view::current_host().is_some() {
            install_stylesheet(PICKER_SHEET, PICKER_CSS);
        }

        let revision = self.0.revision;
        let is_save = self.read(|state| {
            state
                .picker
                .as_ref()
                .is_some_and(|picker| picker.options.is_save())
        });
        let show_read_only = self.read(|state| {
            state.picker.as_ref().is_some_and(|picker| {
                !picker.options.directory
                    && !picker.options.is_save()
                    && self.0.picker_result.is_none()
            })
        });
        let filters: Vec<(String, String)> = self.read(|state| {
            state
                .picker
                .as_ref()
                .map(|picker| {
                    picker
                        .filters()
                        .iter()
                        .enumerate()
                        .map(|(idx, filter)| {
                            let label = filter
                                .label
                                .clone()
                                .unwrap_or_else(|| i18n::text("picker-all-files"));
                            (idx.to_string(), label)
                        })
                        .collect()
                })
                .unwrap_or_default()
        });
        let show_filters = filters.len() > 1
            || self.read(|state| {
                state
                    .picker
                    .as_ref()
                    .is_some_and(|picker| !picker.options.directory && !picker.filters().is_empty())
            });

        let choices_data = self.read(|state| {
            state
                .picker
                .as_ref()
                .map(|picker| picker.choices.clone())
                .unwrap_or_default()
        });

        let confirm_label = self.read(|state| {
            let Some(picker) = state.picker.as_ref() else {
                return i18n::text("picker-confirm");
            };
            if let Some(custom) = picker
                .options
                .portal
                .as_ref()
                .and_then(|portal| portal.accept_label.as_deref())
            {
                let cleaned = strip_mnemonic(custom);
                if !cleaned.is_empty() {
                    return cleaned;
                }
            }
            if picker.options.is_save() {
                i18n::text("picker-save")
            } else if picker.options.directory {
                i18n::text("picker-confirm-directory")
            } else {
                i18n::text("picker-confirm")
            }
        });
        let cancel_label = i18n::text("picker-cancel");

        let save_name = self.0.save_name;
        let save_input_node = NodeRef::new();
        let save_composing = Rc::new(Cell::new(false));
        let save_focus_browser = self.clone();
        if is_save {
            let save_effect = RenderEffect::new(move |prev: Option<bool>| {
                let mounted = save_input_node.get().is_some();
                if mounted && prev != Some(true) {
                    save_input_node.focus();
                    let name = save_focus_browser.0.save_name.get_untracked();
                    let stem_end = save_stem_end(&name);
                    save_input_node.set_value(&name);
                    save_input_node.set_selection(0..stem_end);
                }
                mounted
            });
            on_cleanup_local(move || drop(save_effect));
        }

        let filter_browser = self.clone();
        let active_filter_value = Binding::controlled(
            Signal::derive_local(move || {
                let _ = revision.get();
                filter_browser.read(|state| {
                    state
                        .picker
                        .as_ref()
                        .map_or_else(|| "0".to_owned(), |picker| picker.active_filter.to_string())
                })
            }),
            {
                let browser = self.clone();
                move |next: String| {
                    if let Ok(index) = next.parse::<usize>() {
                        browser.select_picker_filter(index);
                    }
                }
            },
        );

        let ro_read_browser = self.clone();
        let ro_write_browser = self.clone();
        let read_only_binding = Binding::controlled(
            Signal::derive_local(move || {
                let _ = revision.get();
                if ro_read_browser
                    .read(|state| state.picker.as_ref().is_some_and(|picker| picker.read_only))
                {
                    Checked::Yes
                } else {
                    Checked::No
                }
            }),
            move |checked: Checked| {
                ro_write_browser.update(|state| {
                    if let Some(picker) = state.picker.as_mut() {
                        picker.read_only = checked == Checked::Yes;
                    }
                });
                ro_write_browser.changed();
            },
        );

        let disabled_confirm = {
            let browser = self.clone();
            Signal::derive_local(move || {
                let _ = revision.get();
                browser.read(|state| {
                    state
                        .picker
                        .as_ref()
                        .is_none_or(|picker| picker.submitting || picker.completing)
                })
            })
        };

        let browser_accept_input = self.clone();
        let browser_cancel_input = self.clone();
        let start_composing = save_composing.clone();
        let preedit_composing = save_composing.clone();
        let commit_composing = save_composing.clone();
        let end_composing = save_composing.clone();
        let blur_composing = save_composing.clone();

        let browser_cancel_btn = self.clone();
        let browser_accept_btn = self.clone();
        let browser_choice_bool = self.clone();
        let browser_choice_select = self.clone();

        let choice_views: Vec<AnyView> = choices_data
            .into_iter()
            .map(|choice| {
                let choice_id = choice.id.clone();
                let label = strip_mnemonic(&choice.label);
                if choice.options.is_empty() {
                    let read_b = browser_choice_bool.clone();
                    let write_b = browser_choice_bool.clone();
                    let read_id = choice_id.clone();
                    let write_id = choice_id;
                    let checked = Binding::controlled(
                        Signal::derive_local(move || {
                            let _ = revision.get();
                            let active = read_b.read(|state| {
                                state.picker.as_ref().is_some_and(|picker| {
                                    picker
                                        .choices
                                        .iter()
                                        .find(|c| c.id == read_id)
                                        .is_some_and(|c| c.selected == "true")
                                })
                            });
                            if active { Checked::Yes } else { Checked::No }
                        }),
                        move |next: Checked| {
                            write_b.set_picker_choice(
                                &write_id,
                                if next == Checked::Yes {
                                    "true"
                                } else {
                                    "false"
                                },
                            );
                        },
                    );
                    AnyView::new(view! {
                        row(class = "gds-files-picker-footer__controls") {
                            Checkbox(checked = checked) {}
                            text { {label} }
                        }
                    })
                } else {
                    let read_b = browser_choice_select.clone();
                    let write_b = browser_choice_select.clone();
                    let read_id = choice_id.clone();
                    let write_id = choice_id;
                    let selected_binding = Binding::controlled(
                        Signal::derive_local(move || {
                            let _ = revision.get();
                            read_b.read(|state| {
                                state
                                    .picker
                                    .as_ref()
                                    .and_then(|picker| {
                                        picker
                                            .choices
                                            .iter()
                                            .find(|c| c.id == read_id)
                                            .map(|c| c.selected.clone())
                                    })
                                    .unwrap_or_default()
                            })
                        }),
                        move |next: String| {
                            write_b.set_picker_choice(&write_id, &next);
                        },
                    );
                    let options = Signal::stored_local(choice.options);
                    AnyView::new(view! {
                        row(class = "gds-files-picker-footer__controls") {
                            text(class = "gds-files-picker-footer__label") { {label.clone()} }
                            Select(value = selected_binding) {
                                SelectTrigger(
                                    size = SelectTriggerSize::Sm,
                                    label = label,
                                ) {
                                    SelectValue {}
                                }
                                SelectContent {
                                    for opt in move || options.get(), key = |opt| opt.0.clone() {
                                        SelectItem(value = opt.0) {
                                            {strip_mnemonic(&opt.1)}
                                        }
                                    }
                                }
                            }
                        }
                    })
                }
            })
            .collect();

        let filters_signal = Signal::stored_local(filters);
        let file_name_label = i18n::text("picker-file-name");
        let file_type_label = i18n::text("picker-file-type");
        let read_only_label = i18n::text("picker-read-only");

        AnyView::new(view! {
            column(class = "gds-files-picker-footer") {
                {is_save.then(|| view! {
                    row(class = "gds-files-picker-footer__row") {
                        row(class = "gds-files-picker-footer__field") {
                            text(class = "gds-files-picker-footer__label") { {file_name_label.clone()} }
                            Input(
                                value = save_name,
                                label = file_name_label.clone(),
                                node_ref = save_input_node,
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
                                    if save_composing.get() {
                                        return;
                                    }
                                    if event.key == Key::Named(NamedKey::Enter)
                                        && !event.modifiers.control()
                                        && !event.modifiers.alt()
                                        && !event.modifiers.meta()
                                    {
                                        event.prevent_default();
                                        event.stop_propagation();
                                        browser_accept_input.accept_picker();
                                    } else if event.key == Key::Named(NamedKey::Escape) {
                                        event.prevent_default();
                                        event.stop_propagation();
                                        browser_cancel_input.cancel_picker();
                                    }
                                },
                            ) {}
                        }
                    }
                })}
                row(class = "gds-files-picker-footer__row") {
                    row(class = "gds-files-picker-footer__controls") {
                        {show_filters.then(|| view! {
                            row(class = "gds-files-picker-footer__controls") {
                                text(class = "gds-files-picker-footer__label") { {file_type_label.clone()} }
                                Select(value = active_filter_value) {
                                    SelectTrigger(
                                        size = SelectTriggerSize::Sm,
                                        label = file_type_label.clone(),
                                    ) {
                                        SelectValue {}
                                    }
                                    SelectContent {
                                        for item in move || filters_signal.get(), key = |item| item.0.clone() {
                                            SelectItem(value = item.0) {
                                                {item.1}
                                            }
                                        }
                                    }
                                }
                            }
                        })}
                        {show_read_only.then(|| view! {
                            row(class = "gds-files-picker-footer__controls") {
                                Checkbox(
                                    checked = read_only_binding,
                                    on_change = UnsyncCallback::new(|_| {}),
                                ) {}
                                text { {read_only_label.clone()} }
                            }
                        })}
                        {choice_views}
                    }
                    row(class = "gds-files-picker-footer__actions") {
                        Button(
                            variant = ButtonVariant::Outline,
                            on:click = move |_| browser_cancel_btn.cancel_picker(),
                        ) {
                            {cancel_label.clone()}
                        }
                        Button(
                            variant = ButtonVariant::Default,
                            disabled = disabled_confirm,
                            on:click = move |_| browser_accept_btn.accept_picker(),
                        ) {
                            {confirm_label.clone()}
                        }
                    }
                }
            }
        })
    }

    pub(super) fn activate_picker_path(&self, path: PathBuf) {
        let already_busy = self.read(|state| {
            state
                .picker
                .as_ref()
                .is_none_or(|picker| picker.submitting || picker.completing)
        });
        if already_busy {
            return;
        }
        let check_path = path.clone();
        self.blocking(
            "Inspect picker selection",
            move || Ok(inspect_activated_path(&check_path)),
            move |browser, result| match result {
                Ok(Ok(ActivatedPath::Directory)) => {
                    let location = browser.local_location(&path);
                    browser.open(location);
                }
                Ok(Ok(ActivatedPath::File)) => {
                    let is_save = browser.read(|state| {
                        state
                            .picker
                            .as_ref()
                            .is_some_and(|picker| picker.options.is_save())
                    });
                    if is_save {
                        if let Some(name) = path.file_name() {
                            let lossy = name.to_string_lossy().into_owned();
                            let raw = name.to_os_string();
                            browser.0.save_name.set(lossy);
                            browser.update(|state| {
                                if let Some(picker) = state.picker.as_mut() {
                                    picker.save_name = Some(raw);
                                }
                            });
                        }
                        browser.accept_picker();
                    } else {
                        browser.validate_and_complete_picker(vec![path]);
                    }
                }
                Ok(Err(failure)) => {
                    browser.report_notice(format_validation_failure(
                        failure.message_id,
                        failure.detail.as_deref(),
                    ));
                }
                Err(error) => browser.report_notice(error),
            },
        );
    }

    pub(super) fn accept_picker(&self) {
        enum AcceptPlan {
            Save {
                mode: chooser::Mode,
                target: PathBuf,
            },
            SaveFiles {
                mode: chooser::Mode,
                folder: PathBuf,
            },
            Open {
                paths: Vec<PathBuf>,
            },
        }

        let plan = {
            let mut state = self.0.state.borrow_mut();
            let current_dir = state.tab().navigation.path().map(Path::to_path_buf);
            let dir_contents = matches!(state.tab().contents, Contents::Directory);
            let selected = state.tab().selected();
            let Some(picker) = state.picker.as_mut() else {
                return;
            };
            if picker.submitting || picker.completing {
                return;
            }
            if picker.options.is_save() {
                let Some(dir) = current_dir else {
                    return;
                };
                let typed = self.0.save_name.get_untracked();
                let file_name = if let Some(original) = picker.save_name.as_ref()
                    && original.to_string_lossy() == typed
                {
                    original.clone()
                } else {
                    OsString::from(typed)
                };
                if !Options::valid_name(&file_name) {
                    drop(state);
                    self.report_notice(i18n::text("rename-error-invalid"));
                    return;
                }
                let target = dir.join(file_name);
                let mode = picker
                    .options
                    .portal
                    .as_ref()
                    .map(|portal| portal.mode.clone())
                    .unwrap_or(chooser::Mode::Save {
                        name: OsString::new(),
                    });
                picker.submitting = true;
                AcceptPlan::Save { mode, target }
            } else if let Some(chooser::Mode::SaveFiles { names }) = picker
                .options
                .portal
                .as_ref()
                .map(|portal| &portal.mode)
                .cloned()
            {
                let folder = selected
                    .iter()
                    .find_map(|row| row.path().map(Path::to_path_buf))
                    .or(current_dir);
                let Some(folder) = folder else {
                    return;
                };
                picker.submitting = true;
                AcceptPlan::SaveFiles {
                    mode: chooser::Mode::SaveFiles { names },
                    folder,
                }
            } else {
                let mut paths: Vec<PathBuf> = selected
                    .iter()
                    .filter_map(|row| row.path().map(Path::to_path_buf))
                    .collect();
                if paths.is_empty()
                    && picker.options.directory
                    && let Some(dir) = current_dir
                    && dir_contents
                {
                    paths.push(dir);
                }
                picker.submitting = true;
                AcceptPlan::Open { paths }
            }
        };

        match plan {
            AcceptPlan::Save { mode, target } => {
                self.blocking(
                    "Prepare save target",
                    move || Ok(prepare_save_targets(&mode, vec![target])),
                    |browser, result| {
                        browser.update(|state| {
                            if let Some(picker) = state.picker.as_mut() {
                                picker.submitting = false;
                            }
                        });
                        match result {
                            Ok(Ok(SaveDecision::Ready(paths))) => {
                                browser.finish_picker_selection(paths);
                            }
                            Ok(Ok(SaveDecision::ConfirmOverwrite(paths))) => {
                                let overwrite_browser = browser.clone();
                                browser.confirm_labelled(
                                    i18n::text("picker-overwrite-title"),
                                    i18n::text("picker-overwrite-message"),
                                    i18n::text("picker-save"),
                                    move || {
                                        overwrite_browser.finish_picker_selection(paths.clone());
                                    },
                                );
                            }
                            Ok(Err(failure)) => {
                                browser.report_notice(format_validation_failure(
                                    failure.message_id,
                                    failure.detail.as_deref(),
                                ));
                            }
                            Err(error) => browser.report_notice(error),
                        }
                    },
                );
            }
            AcceptPlan::SaveFiles { mode, folder } => {
                self.blocking(
                    "Prepare save folder targets",
                    move || Ok(prepare_save_targets(&mode, vec![folder])),
                    |browser, result| {
                        browser.update(|state| {
                            if let Some(picker) = state.picker.as_mut() {
                                picker.submitting = false;
                            }
                        });
                        match result {
                            Ok(Ok(SaveDecision::Ready(paths)))
                            | Ok(Ok(SaveDecision::ConfirmOverwrite(paths))) => {
                                browser.finish_picker_selection(paths);
                            }
                            Ok(Err(failure)) => {
                                browser.report_notice(format_validation_failure(
                                    failure.message_id,
                                    failure.detail.as_deref(),
                                ));
                            }
                            Err(error) => browser.report_notice(error),
                        }
                    },
                );
            }
            AcceptPlan::Open { paths } => self.validate_and_complete_picker(paths),
        }
    }

    fn validate_and_complete_picker(&self, paths: Vec<PathBuf>) {
        let check = {
            let mut state = self.0.state.borrow_mut();
            let accepted_in_table: Vec<PathBuf> = {
                let tab = state.tab();
                let mut accepted = Vec::new();
                if let Some(picker) = state.picker.as_ref() {
                    for row in &tab.rows {
                        if let super::model::Row::File(entry) = row
                            && picker.accepts_entry(entry)
                        {
                            accepted.push(entry.path.clone());
                        }
                    }
                }
                accepted
            };
            let Some(picker) = state.picker.as_mut() else {
                return;
            };
            picker.submitting = true;
            picker.selection_check(accepted_in_table)
        };
        let candidate_paths = paths.clone();
        self.blocking(
            "Validate picker selection",
            move || Ok(check.validate(&candidate_paths)),
            move |browser, result| {
                browser.update(|state| {
                    if let Some(picker) = state.picker.as_mut() {
                        picker.submitting = false;
                    }
                });
                match result {
                    Ok(Ok(())) => browser.finish_picker_selection(paths),
                    Ok(Err(failure)) => {
                        browser.report_notice(format_validation_failure(
                            failure.message_id,
                            failure.detail.as_deref(),
                        ));
                    }
                    Err(error) => browser.report_notice(error),
                }
            },
        );
    }

    fn finish_picker_selection(&self, paths: Vec<PathBuf>) {
        if let Some(callback) = &self.0.picker_result {
            self.update(|state| {
                if let Some(picker) = state.picker.as_mut() {
                    picker.completing = true;
                }
            });
            callback(paths);
            if let Some(window) = &self.0.window {
                window.close();
            }
            return;
        }
        let outcome = self.update(|state| {
            let picker = state.picker.as_mut()?;
            if picker.completing {
                return None;
            }
            Some(picker.complete(&paths))
        });
        match outcome {
            Some(Ok(())) => {
                if let Some(window) = &self.0.window {
                    window.close();
                }
            }
            Some(Err(error)) => {
                self.report_notice(error.to_string());
                if let Some(window) = &self.0.window {
                    window.close();
                }
            }
            None => {}
        }
    }

    pub(super) fn cancel_picker(&self) {
        let picker_result = self.0.picker_result.is_some();
        let should_close = self.update(|state| {
            state
                .picker
                .as_mut()
                .is_some_and(|picker| picker.cancel() || picker_result)
        });
        if should_close && let Some(window) = &self.0.window {
            window.close();
        }
    }

    pub(super) fn picker_key(&self, event: &zgui::vocab::KeyEvent) -> bool {
        if self.read(|state| state.picker.is_none()) {
            return false;
        }
        match event.key {
            Key::Named(NamedKey::Escape) => {
                self.cancel_picker();
                true
            }
            Key::Named(NamedKey::Enter) => {
                self.accept_picker();
                true
            }
            _ => false,
        }
    }

    fn select_picker_filter(&self, index: usize) {
        let active_id = self.update(|state| {
            let changed = state
                .picker
                .as_mut()
                .is_some_and(|picker| picker.select_filter(index));
            if changed { Some(state.tab().id) } else { None }
        });
        if let Some(id) = active_id {
            self.reproject(id);
            self.enrich_directory(id);
        }
    }

    fn set_picker_choice(&self, id: &str, value: &str) {
        let updated = self.update(|state| {
            let Some(picker) = state.picker.as_mut() else {
                return false;
            };
            let Some(choice) = picker.choices.iter_mut().find(|c| c.id == id) else {
                return false;
            };
            if choice.selected == value {
                return false;
            }
            choice.selected = value.to_owned();
            true
        });
        if updated {
            self.changed();
        }
    }
}

fn format_validation_failure(message_id: &str, detail: Option<&str>) -> String {
    let base = i18n::text(message_id);
    match detail {
        Some(detail) if !detail.is_empty() => format!("{base} ({detail})"),
        _ => base,
    }
}

fn strip_mnemonic(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '_' {
            if chars.peek() == Some(&'_') {
                out.push('_');
                chars.next();
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn save_stem_end(name: &str) -> usize {
    if let Some(stem) = Path::new(name).file_stem().and_then(|s| s.to_str())
        && !stem.is_empty()
        && name.starts_with(stem)
        && name.as_bytes().get(stem.len()) == Some(&b'.')
    {
        return stem.len();
    }
    name.len()
}
