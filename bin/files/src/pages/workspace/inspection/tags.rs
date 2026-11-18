//! Chip creation, removal and draft typing share the original editing history.

mod history;

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

use gds::i18n;
use gds::service::file_tags;
use gds::ui::button::{ButtonSize, ButtonVariant};
use gds::ui::{IconButtonProps, IconName};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, Signal};
use zgui::view::{FrameHandle, current_host, install_stylesheet, request_frame};
use zgui::vocab::{ImeEvent, Key, Modifiers, NamedKey};
use zgui::{css, view};
use zgui_ui::{ButtonProps, InputProps};
use zgui_ui_primitives::Binding;

use super::super::ui::{Browser, WeakBrowser};
use history::{History, Snapshot};

const TAGS_SHEET: &str = "gds-files-workspace-tags";

const TAGS_CSS: &str = css!(
    r#"
    .gds-files-tag-editor {
        gap: 10px;
    }

    .gds-files-tag-editor__chips {
        display: flex;
        flex-direction: row;
        flex-wrap: wrap;
        gap: 6px;
        min-height: 28px;
    }

    .gds-files-tag-chip {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 4px;
        padding: 2px 6px 2px 10px;
        border-radius: 999px;
        background: var(--gds-surface);
        border: 1px solid var(--gds-border);
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-foreground);
    }

    .gds-files-tag-chip--empty {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-muted-foreground);
    }

    .gds-files-tag-editor__row {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 8px;
    }

    .gds-files-tag-editor__input {
        flex: 1 1 0%;
        min-width: 0;
    }

    .gds-files-tag-editor__status {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-muted-foreground);
    }

    .gds-files-tag-editor__error {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-error);
    }
    "#
);

fn install_tags_stylesheet() {
    if current_host().is_some() {
        install_stylesheet(TAGS_SHEET, TAGS_CSS);
    }
}

struct TagEditorInner {
    browser: WeakBrowser,
    path: PathBuf,
    input_ref: NodeRef,
    draft: RwSignal<String, LocalStorage>,
    tags: RwSignal<Vec<String>, LocalStorage>,
    original: RefCell<Vec<String>>,
    history: RefCell<History>,
    loading: RwSignal<bool, LocalStorage>,
    saving: RwSignal<bool, LocalStorage>,
    load_failed: RwSignal<bool, LocalStorage>,
    composing: RwSignal<bool, LocalStorage>,
    error: RwSignal<Option<String>, LocalStorage>,
    pending_focus: RefCell<Option<FrameHandle>>,
}

#[derive(Clone)]
pub(in crate::pages::workspace) struct TagEditor(Rc<TagEditorInner>);

impl TagEditor {
    pub(super) fn new(browser: &Browser, path: PathBuf) -> Self {
        let inner = Rc::new(TagEditorInner {
            browser: browser.downgrade(),
            path: path.clone(),
            input_ref: NodeRef::new(),
            draft: RwSignal::new_local(String::new()),
            tags: RwSignal::new_local(Vec::new()),
            original: RefCell::new(Vec::new()),
            history: RefCell::new(History::default()),
            loading: RwSignal::new_local(true),
            saving: RwSignal::new_local(false),
            load_failed: RwSignal::new_local(false),
            composing: RwSignal::new_local(false),
            error: RwSignal::new_local(None),
            pending_focus: RefCell::new(None),
        });
        let editor = Self(inner);
        let weak = Rc::downgrade(&editor.0);
        let read_path = path;
        browser.blocking(
            "Read file tags",
            move || file_tags::read(&read_path).map_err(|err| err.to_string()),
            move |_, result| {
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let target = Self(inner);
                match result {
                    Ok(loaded) => {
                        *target.0.original.borrow_mut() = loaded.clone();
                        let mut fresh = History::default();
                        fresh.current = Snapshot {
                            tags: loaded.clone(),
                            draft: String::new(),
                            selection: 0..0,
                            reversed: false,
                        };
                        *target.0.history.borrow_mut() = fresh;
                        target.0.tags.set(loaded);
                        target.0.loading.set(false);
                        target.0.load_failed.set(false);
                        target.0.error.set(None);
                        target.focus();
                    }
                    Err(message) => {
                        target.0.loading.set(false);
                        target.0.load_failed.set(true);
                        target.0.error.set(Some(message));
                    }
                }
            },
        );
        editor
    }

    fn editable(&self) -> bool {
        !self.0.loading.get_untracked()
            && !self.0.load_failed.get_untracked()
            && !self.0.saving.get_untracked()
    }

    pub(in crate::pages::workspace) fn is_saving(&self) -> bool {
        self.0.saving.get()
    }

    pub(super) fn available(&self) -> bool {
        !self.0.loading.get() && !self.0.load_failed.get()
    }

    pub(super) fn focus(&self) {
        if self.0.input_ref.is_bound() {
            self.0.input_ref.focus();
            return;
        }
        let weak = Rc::downgrade(&self.0);
        if let Some(browser) = self.0.browser.upgrade() {
            let handle = browser.0.owner.with(|| {
                request_frame(move |_| {
                    if let Some(inner) = weak.upgrade() {
                        inner.input_ref.focus();
                    }
                })
            });
            *self.0.pending_focus.borrow_mut() = Some(handle);
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.0.path
    }

    fn record_input(&self, value: String) {
        self.0.draft.set(value.clone());
        if !self.editable() || self.0.composing.get_untracked() {
            return;
        }
        let selection = self
            .0
            .input_ref
            .selection()
            .unwrap_or(value.len()..value.len());
        let tags = self.0.tags.get_untracked();
        {
            let mut history = self.0.history.borrow_mut();
            history.record(
                Snapshot {
                    tags,
                    draft: value,
                    selection,
                    reversed: false,
                },
                true,
            );
        }
        self.0.error.set(None);
    }

    fn restore_snapshot(&self, snapshot: Snapshot) {
        self.0.tags.set(snapshot.tags);
        self.0.draft.set(snapshot.draft.clone());
        self.0.input_ref.set_value(&snapshot.draft);
        self.0.input_ref.set_selection(snapshot.selection);
        self.0.error.set(None);
    }

    fn add(&self) -> Result<bool, String> {
        if !self.editable() {
            return Ok(false);
        }
        let raw = self.0.draft.get_untracked();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            self.0.draft.set(String::new());
            self.0.input_ref.set_value("");
            self.0.input_ref.set_selection(0..0);
            self.0.history.borrow_mut().break_typing();
            return Ok(false);
        }
        if let Err(err) = file_tags::validate(trimmed) {
            let msg = err.to_string();
            self.0.error.set(Some(msg.clone()));
            return Err(msg);
        }
        let mut tags = self.0.tags.get_untracked();
        if !tags.iter().any(|existing| existing == trimmed) {
            tags.push(trimmed.to_owned());
        }
        {
            let mut history = self.0.history.borrow_mut();
            history.record(
                Snapshot {
                    tags: tags.clone(),
                    draft: String::new(),
                    selection: 0..0,
                    reversed: false,
                },
                false,
            );
        }
        self.0.tags.set(tags);
        self.0.draft.set(String::new());
        self.0.input_ref.set_value("");
        self.0.input_ref.set_selection(0..0);
        self.0.error.set(None);
        Ok(true)
    }

    fn remove(&self, tag: &str) {
        if !self.editable() {
            return;
        }
        let mut tags = self.0.tags.get_untracked();
        let before = tags.len();
        tags.retain(|item| item != tag);
        if tags.len() == before {
            return;
        }
        let draft = self.0.draft.get_untracked();
        let selection = self
            .0
            .input_ref
            .selection()
            .unwrap_or(draft.len()..draft.len());
        {
            let mut history = self.0.history.borrow_mut();
            history.record(
                Snapshot {
                    tags: tags.clone(),
                    draft,
                    selection,
                    reversed: false,
                },
                false,
            );
        }
        self.0.tags.set(tags);
        self.0.error.set(None);
    }

    fn handle_key(&self, event: &mut zgui::view::EventCx<'_, zgui::view::events::KeyDown>) {
        if self.0.composing.get_untracked() || !self.editable() {
            return;
        }
        let modifiers = event.modifiers;
        let has_chord = modifiers.contains(Modifiers::CONTROL)
            && !modifiers.contains(Modifiers::ALT)
            && !modifiers.contains(Modifiers::META);

        if has_chord && let Key::Character(ch) = &event.key {
            let redo = if ch.eq_ignore_ascii_case("z") {
                modifiers.contains(Modifiers::SHIFT)
            } else {
                ch.eq_ignore_ascii_case("y") && !modifiers.contains(Modifiers::SHIFT)
            };
            if ch.eq_ignore_ascii_case("z") || (redo && ch.eq_ignore_ascii_case("y")) {
                let next = {
                    let mut history = self.0.history.borrow_mut();
                    history.step(redo).then(|| history.current.clone())
                };
                if let Some(snapshot) = next {
                    self.restore_snapshot(snapshot);
                }
                event.prevent_default();
                event.stop_propagation();
                return;
            }
        }

        if !modifiers.is_empty() {
            return;
        }

        match &event.key {
            Key::Named(NamedKey::Enter) => {
                if self.add().is_ok_and(|added| added) || self.0.error.get_untracked().is_some() {
                    event.prevent_default();
                    event.stop_propagation();
                }
            }
            Key::Named(NamedKey::Backspace) => {
                if self.0.draft.get_untracked().is_empty()
                    && let Some(last) = self.0.tags.get_untracked().last().cloned()
                {
                    self.remove(&last);
                    event.prevent_default();
                    event.stop_propagation();
                }
            }
            _ => {}
        }
    }

    pub(super) fn save(&self, complete: impl FnOnce() + 'static) {
        if !self.editable() {
            return;
        }
        if self.add().is_err() {
            return;
        }
        let Some(browser) = self.0.browser.upgrade() else {
            return;
        };
        let original = self.0.original.borrow().clone();
        let edited = self.0.tags.get_untracked();
        let save_path = self.0.path.clone();
        let refresh_path = self.0.path.clone();
        let weak = Rc::downgrade(&self.0);

        self.0.saving.set(true);
        self.0.error.set(None);

        let done = RefCell::new(Some(Box::new(complete) as Box<dyn FnOnce()>));
        browser.blocking(
            "Save file tags",
            move || {
                file_tags::apply_edit(&save_path, &original, &edited)
                    .map(|()| edited)
                    .map_err(|err| err.to_string())
            },
            move |browser, result| {
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let target = Self(inner);
                target.0.saving.set(false);
                match result {
                    Ok(saved) => {
                        *target.0.original.borrow_mut() = saved;
                        browser.report_success(i18n::text_untracked("tags-saved"));
                        browser.refresh_affected(&[refresh_path]);
                        browser.refresh_tag_tabs();
                        if let Some(callback) = done.borrow_mut().take() {
                            callback();
                        }
                    }
                    Err(message) => {
                        target.0.error.set(Some(message));
                    }
                }
            },
        );
    }

    fn chip(&self, tag: String) -> AnyView {
        let display = tag.clone();
        let remove_tag = tag;
        let editor = self.clone();
        let disabled = Signal::derive_local(move || editor.is_saving());
        let remove_editor = self.clone();

        AnyView::new(view! {
            row(class = "gds-files-tag-chip") {
                text {{display}}
                IconButton(
                    icon = IconName::Close,
                    label = i18n::text("tags-remove"),
                    disabled = disabled,
                    on:click = move |_| {
                        remove_editor.remove(&remove_tag);
                        remove_editor.focus();
                    },
                )
            }
        })
    }

    pub(super) fn fields(&self) -> AnyView {
        install_tags_stylesheet();
        let tags_sig = self.0.tags;
        let draft_sig = self.0.draft;
        let loading_sig = self.0.loading;
        let saving_sig = self.0.saving;
        let load_failed_sig = self.0.load_failed;
        let composing_sig = self.0.composing;
        let error_sig = self.0.error;
        let input_ref = self.0.input_ref;

        let disabled = Signal::derive_local(move || {
            loading_sig.get() || saving_sig.get() || load_failed_sig.get()
        });
        let invalid = Signal::derive_local(move || error_sig.get().is_some());

        let chips_editor = self.clone();
        let input_editor = self.clone();
        let key_editor = self.clone();
        let add_editor = self.clone();

        AnyView::new(view! {
            column(class = "gds-files-tag-editor") {
                row(class = "gds-files-tag-editor__chips") {
                    {move || {
                        let tags = tags_sig.get();
                        if tags.is_empty() && !loading_sig.get() {
                            vec![AnyView::new(view! {
                                text(class = "gds-files-tag-chip--empty") {{i18n::text("tags-none")}}
                            })]
                        } else {
                            tags.into_iter().map(|tag| chips_editor.chip(tag)).collect()
                        }
                    }}
                }
                row(class = "gds-files-tag-editor__row") {
                    Input(
                        class = "gds-files-tag-editor__input",
                        node_ref = input_ref,
                        placeholder = i18n::text("tags-add-placeholder"),
                        disabled = disabled,
                        invalid = invalid,
                        value = Binding::controlled(draft_sig, move |next| {
                            input_editor.record_input(next);
                        }),
                        on:ime_start = move |_| composing_sig.set(true),
                        on:ime_preedit = move |ev| {
                            let active = match &**ev {
                                ImeEvent::Preedit { text, .. } => !text.is_empty(),
                                _ => true,
                            };
                            composing_sig.set(active);
                        },
                        on:ime_commit = move |_| composing_sig.set(false),
                        on:ime_end = move |_| composing_sig.set(false),
                        on:key_down = move |ev| key_editor.handle_key(ev),
                    )
                    Button(
                        variant = ButtonVariant::Outline,
                        size = ButtonSize::Sm,
                        disabled = disabled,
                        on:click = move |_| {
                            let _ = add_editor.add();
                            add_editor.focus();
                        },
                    ) {{i18n::text("tags-add")}}
                }
                {move || {
                    if loading_sig.get() {
                        Some(AnyView::new(view! {
                            text(class = "gds-files-tag-editor__status") {{i18n::text("tags-loading")}}
                        }))
                    } else if saving_sig.get() {
                        Some(AnyView::new(view! {
                            text(class = "gds-files-tag-editor__status") {{i18n::text("tags-saving")}}
                        }))
                    } else {
                        None
                    }
                }}
                {move || {
                    error_sig.get().map(|err| {
                        view! {
                            text(class = "gds-files-tag-editor__error") {{err}}
                        }
                        .into_view()
                    })
                }}
            }
        })
    }
}
