//! Multi-tab embedded terminal dock synced with active folder tabs.

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};

use gds::i18n;
use gds::service::terminal::SpawnOptions;
use gds::t;
use gds::ui::button::{ButtonSize, ButtonVariant};
use gds::ui::feedback::tooltip::HintTooltipProps;
use gds::ui::navigation::{
    NavItemActionsProps, NavItemContentProps, NavItemDetailsProps, NavItemHeaderProps,
    NavItemLabelProps, NavItemProps,
};
use gds::ui::sidebar::SidebarPresentation;
use gds::ui::terminal::{Event as TerminalEvent, Terminal, TerminalViewProps};
use gds::ui::{IconButtonProps, IconName, IconProps};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, RwSignal, Signal, on_cleanup_local, spawn_local};
use zgui::view::{FrameHandle, current_host, install_stylesheet, request_frame};
use zgui::vocab::{Key, KeyEvent, KeyLocation, Modifiers, NamedKey, PhysicalKey, Role};
use zgui::{css, view};
use zgui_ui::{ButtonProps, InputProps};
use zgui_ui_primitives::Binding;

use super::ui::Browser;
use crate::entities::preferences::Preferences;
use crate::shared::ui::actions::Command;

const MIN_TERMINAL_HEIGHT: f32 = 140.0;
const DEFAULT_TERMINAL_HEIGHT: f32 = 240.0;
const MIN_CONTENT_HEIGHT: f32 = 160.0;
const RAIL_MIN_WIDTH: f32 = 160.0;

const TERMINALS_SHEET: &str = "gds-files-workspace-terminals";

const TERMINALS_CSS: &str = css!(
    r#"
    .gds-files-terminal-dock {
        display: flex;
        flex-direction: column;
        width: 100%;
        flex-shrink: 0;
        min-height: 0;
        background: var(--gds-surface);
        border-top: 1px solid var(--gds-border);
        overflow: hidden;
    }

    .gds-files-terminal-handle {
        height: 7px;
        width: 100%;
        flex-shrink: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        cursor: ns-resize;
        background: transparent;
    }

    .gds-files-terminal-handle__line {
        width: 100%;
        height: 1px;
        background: var(--gds-border);
    }

    .gds-files-terminal-handle:hover .gds-files-terminal-handle__line,
    .gds-files-terminal-handle[data-dragging="true"] .gds-files-terminal-handle__line {
        height: 2px;
        background: var(--gds-primary);
    }

    .gds-files-terminal-body {
        display: flex;
        flex-direction: row;
        flex: 1 1 0%;
        min-width: 0;
        min-height: 0;
    }

    .gds-files-terminal-rail {
        display: flex;
        flex-direction: column;
        flex-shrink: 0;
        min-width: 0;
        min-height: 0;
        overflow: hidden;
        background: var(--gds-sidebar-bg, var(--gds-sidebar));
        border-right: 1px solid var(--gds-border);
    }

    .gds-files-terminal-rail__header {
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        padding: 4px 6px;
        gap: 4px;
        border-bottom: 1px solid var(--gds-border);
        flex-shrink: 0;
    }

    .gds-files-terminal-rail__title {
        font-size: var(--gds-font-size-xs, 12px);
        font-weight: 600;
        color: var(--gds-muted-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        padding-left: 4px;
    }

    .gds-files-terminal-rail__actions {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 2px;
        flex-shrink: 0;
    }

    .gds-files-terminal-rail__list {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0;
        min-height: 0;
        overflow-y: auto;
        overflow-x: hidden;
        padding: 4px;
        gap: 2px;
    }

    .gds-files-terminal-rail__list .zui-tooltip__trigger {
        display: flex;
        flex: 1 1 0%;
        min-width: 0;
        max-width: 100%;
        overflow: hidden;
    }

    .gds-files-terminal-pane {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0;
        min-height: 0;
    }

    .gds-files-terminal-pane__header {
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        padding: 2px 8px;
        height: 30px;
        border-bottom: 1px solid var(--gds-border);
        background: var(--gds-panel);
        flex-shrink: 0;
        gap: 8px;
    }

    .gds-files-terminal-pane__meta {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 8px;
        min-width: 0;
        flex: 1 1 0%;
    }

    .gds-files-terminal-pane__cwd {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-muted-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        min-width: 0;
    }

    .gds-files-terminal-pane__status {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-muted-foreground);
        flex-shrink: 0;
    }

    .gds-files-terminal-pane__status[data-error="true"] {
        color: var(--gds-error);
    }

    .gds-files-terminal-pane__controls {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 2px;
        flex-shrink: 0;
    }

    .gds-files-terminal-pane__surface {
        flex: 1 1 0%;
        min-width: 0;
        min-height: 0;
    }

    .gds-files-terminal-empty {
        flex: 1 1 0%;
        align-items: center;
        justify-content: center;
        gap: 10px;
        padding: 16px;
    }

    .gds-files-terminal-empty__text {
        font-size: var(--gds-font-size-sm, 14px);
        color: var(--gds-muted-foreground);
        text-align: center;
    }
    "#
);

fn install_terminals_stylesheet() {
    if current_host().is_some() {
        install_stylesheet(TERMINALS_SHEET, TERMINALS_CSS);
    }
}

#[derive(Clone)]
struct SessionEntry {
    id: u64,
    terminal: Terminal,
    view_ref: NodeRef,
    started_in: PathBuf,
    custom_title: RwSignal<Option<String>, LocalStorage>,
    renaming: RwSignal<bool, LocalStorage>,
    rename_draft: RwSignal<String, LocalStorage>,
    rename_ref: NodeRef,
    pending_rename_focus: Rc<RefCell<Option<FrameHandle>>>,
    last_synced_dir: RefCell<Option<PathBuf>>,
}

impl SessionEntry {
    fn fallback_title(&self) -> String {
        self.started_in
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.started_in.display().to_string())
    }

    fn display_title(&self) -> String {
        if let Some(custom) = self.custom_title.get()
            && !custom.trim().is_empty()
        {
            return custom;
        }
        if let Some(pty_title) = self.terminal.title().get()
            && !pty_title.trim().is_empty()
        {
            return pty_title;
        }
        self.fallback_title()
    }

    fn display_title_untracked(&self) -> String {
        if let Some(custom) = self.custom_title.get_untracked()
            && !custom.trim().is_empty()
        {
            return custom;
        }
        if let Some(pty_title) = self.terminal.title().get_untracked()
            && !pty_title.trim().is_empty()
        {
            return pty_title;
        }
        self.fallback_title()
    }
}

struct DockInner {
    revision: RwSignal<u64, LocalStorage>,
    next_id: Cell<u64>,
    sessions: RefCell<Vec<SessionEntry>>,
    maximized: RwSignal<bool, LocalStorage>,
    height: RwSignal<f32, LocalStorage>,
    workspace_size: RwSignal<(f32, f32), LocalStorage>,
    dragging: RwSignal<Option<(f32, f32)>, LocalStorage>,
}

#[derive(Clone)]
pub(super) struct Dock {
    active: RwSignal<Option<u64>, LocalStorage>,
    inner: Rc<DockInner>,
}

impl Dock {
    pub(super) fn new() -> Self {
        Self {
            active: RwSignal::new_local(None),
            inner: Rc::new(DockInner {
                revision: RwSignal::new_local(0),
                next_id: Cell::new(1),
                sessions: RefCell::new(Vec::new()),
                maximized: RwSignal::new_local(false),
                height: RwSignal::new_local(DEFAULT_TERMINAL_HEIGHT),
                workspace_size: RwSignal::new_local((1024.0, 640.0)),
                dragging: RwSignal::new_local(None),
            }),
        }
    }

    fn bump(&self) {
        self.inner.revision.update(|rev| *rev = rev.wrapping_add(1));
    }

    fn clamped_height(&self, raw: f32) -> f32 {
        let available = self.inner.workspace_size.get_untracked().1;
        let max_height = (available - MIN_CONTENT_HEIGHT).max(MIN_TERMINAL_HEIGHT);
        raw.clamp(MIN_TERMINAL_HEIGHT, max_height)
    }

    fn effective_height(&self) -> f32 {
        let (_, available) = self.inner.workspace_size.get();
        if self.inner.maximized.get() {
            return (available - 48.0).max(MIN_TERMINAL_HEIGHT);
        }
        let max_height = (available - MIN_CONTENT_HEIGHT).max(MIN_TERMINAL_HEIGHT);
        self.inner
            .height
            .get()
            .clamp(MIN_TERMINAL_HEIGHT, max_height)
    }

    fn find_session(&self, id: u64) -> Option<SessionEntry> {
        self.inner
            .sessions
            .borrow()
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    fn session_snapshot(&self) -> Vec<SessionEntry> {
        self.inner.sessions.borrow().clone()
    }
}

impl Browser {
    fn active_local_directory(&self) -> Option<PathBuf> {
        let location = self.0.state.borrow().tab().navigation.location().clone();
        self.resolve_path(&location).filter(|path| path.is_dir())
    }

    fn default_terminal_cwd(&self) -> Option<PathBuf> {
        let location = self.0.state.borrow().tab().navigation.location().clone();
        self.resolve_path_untracked(&location)
            .filter(|path| path.is_dir())
            .or_else(|| {
                let home = self.0.state.borrow().home.clone();
                home.is_dir().then_some(home)
            })
    }

    fn close_terminal_session(&self, id: u64) {
        let removed_and_next = {
            let mut sessions = self.0.terminals.inner.sessions.borrow_mut();
            let Some(pos) = sessions.iter().position(|entry| entry.id == id) else {
                return;
            };
            let removed = sessions.remove(pos);
            let next_active = if self.0.terminals.active.get_untracked() == Some(id) {
                sessions
                    .get(pos)
                    .or_else(|| pos.checked_sub(1).and_then(|i| sessions.get(i)))
                    .map(|entry| entry.id)
            } else {
                self.0.terminals.active.get_untracked()
            };
            (removed, next_active)
        };

        let (removed, next_active) = removed_and_next;
        self.0.terminals.active.set(next_active);
        self.0.terminals.bump();

        self.0.owner.with(|| {
            drop(spawn_local(removed.terminal.shutdown()));
        });

        if let Some(next_id) = next_active
            && let Some(next_entry) = self.0.terminals.find_session(next_id)
        {
            next_entry.terminal.focus();
        } else if next_active.is_none() {
            self.focus_content();
        }
    }

    fn begin_terminal_rename(&self, id: u64) {
        let Some(entry) = self.0.terminals.find_session(id) else {
            return;
        };
        let current = entry.display_title_untracked();
        entry.rename_draft.set(current);
        entry.renaming.set(true);
        if entry.rename_ref.is_bound() {
            *entry.pending_rename_focus.borrow_mut() = None;
            entry.rename_ref.focus();
            return;
        }
        let target = entry.rename_ref;
        let renaming = entry.renaming;
        let handle = self.0.owner.with(|| {
            request_frame(move |_| {
                if renaming.get_untracked() && target.is_bound() {
                    target.focus();
                }
            })
        });
        *entry.pending_rename_focus.borrow_mut() = Some(handle);
    }

    fn commit_terminal_rename(&self, id: u64, accept: bool) {
        let Some(entry) = self.0.terminals.find_session(id) else {
            return;
        };
        *entry.pending_rename_focus.borrow_mut() = None;
        if !entry.renaming.get_untracked() {
            return;
        }
        if accept {
            let draft = entry.rename_draft.get_untracked();
            let trimmed = draft.trim();
            entry
                .custom_title
                .set((!trimmed.is_empty()).then(|| trimmed.to_owned()));
        }
        entry.renaming.set(false);
        entry.terminal.focus();
    }

    pub(super) fn apply_terminal_preferences(&self, preferences: &Preferences) {
        if !preferences.terminal_enabled() {
            self.0.terminal.set(false);
        }
        if let Some(content_height) = preferences.files.terminal_content_height {
            let available = self.0.terminals.inner.workspace_size.get_untracked().1;
            if available > MIN_CONTENT_HEIGHT + MIN_TERMINAL_HEIGHT {
                let target = self
                    .0
                    .terminals
                    .clamped_height((available - content_height).max(MIN_TERMINAL_HEIGHT));
                self.0.terminals.inner.height.set(target);
            }
        }
        let term_prefs = preferences.terminal.clone();
        let terminals: Vec<Terminal> = self
            .0
            .terminals
            .inner
            .sessions
            .borrow()
            .iter()
            .map(|entry| entry.terminal.clone())
            .collect();
        for terminal in terminals {
            terminal.apply_preferences(term_prefs.clone());
        }
    }

    pub(super) fn toggle_terminal(&self) {
        if !self.0.preferences.snapshot().terminal_enabled() {
            self.0.terminal.set(false);
            return;
        }
        let next = !self.0.terminal.get_untracked();
        self.0.terminal.set(next);
        if next {
            let active_id = self.0.terminals.active.get_untracked();
            let empty = self.0.terminals.inner.sessions.borrow().is_empty();
            if empty {
                self.new_terminal();
            } else if let Some(id) = active_id
                && let Some(entry) = self.0.terminals.find_session(id)
            {
                entry.terminal.focus();
            }
        } else {
            self.0.terminals.inner.maximized.set(false);
            self.focus_content();
        }
    }

    pub(super) fn new_terminal(&self) {
        let Some(cwd) = self.default_terminal_cwd() else {
            self.report_notice(i18n::text_untracked("files-terminal-cwd-unavailable"));
            return;
        };
        self.new_terminal_at(cwd);
    }

    pub(super) fn new_terminal_at(&self, path: PathBuf) {
        if !self.0.preferences.snapshot().terminal_enabled() {
            return;
        }
        if !path.is_dir() {
            self.report_notice(i18n::text_untracked("files-terminal-cwd-unavailable"));
            return;
        }
        self.0.terminal.set(true);

        let term_prefs = self.0.preferences.snapshot().terminal;
        let options = SpawnOptions {
            cwd: path.clone(),
            ..SpawnOptions::default()
        };
        let terminal = match self
            .0
            .owner
            .with(|| Terminal::spawn(options, term_prefs, &self.0.tasks))
        {
            Ok(term) => term,
            Err(err) => {
                self.report_notice(err.to_string());
                return;
            }
        };

        let id = self.0.terminals.inner.next_id.get();
        self.0.terminals.inner.next_id.set(id + 1);

        let weak = self.downgrade();
        terminal.on_event(UnsyncCallback::new(move |event| {
            let Some(browser) = weak.upgrade() else {
                return;
            };
            if let TerminalEvent::Closed(Some(0)) = event {
                browser.close_terminal_session(id);
            }
        }));

        let entry = self.0.owner.with(|| SessionEntry {
            id,
            terminal: terminal.clone(),
            view_ref: NodeRef::new(),
            started_in: path.clone(),
            custom_title: RwSignal::new_local(None),
            renaming: RwSignal::new_local(false),
            rename_draft: RwSignal::new_local(String::new()),
            rename_ref: NodeRef::new(),
            pending_rename_focus: Rc::new(RefCell::new(None)),
            last_synced_dir: RefCell::new(Some(path)),
        });

        {
            self.0.terminals.inner.sessions.borrow_mut().push(entry);
        }
        self.0.terminals.active.set(Some(id));
        self.0.terminals.bump();
        terminal.focus();
    }

    pub(super) fn observe_terminals(&self) {
        if let Some(content_height) = self.0.preferences.snapshot().files.terminal_content_height {
            let available = self.0.terminals.inner.workspace_size.get_untracked().1;
            if available > MIN_CONTENT_HEIGHT + MIN_TERMINAL_HEIGHT {
                let target = self
                    .0
                    .terminals
                    .clamped_height((available - content_height).max(MIN_TERMINAL_HEIGHT));
                self.0.terminals.inner.height.set(target);
            }
        }

        let weak = self.downgrade();
        self.0.owner.with(|| {
            let fx = RenderEffect::new(move |_| {
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                browser.track();
                let open = browser.0.terminal.get();
                let active_id = browser.0.terminals.active.get();
                let _rev = browser.0.terminals.inner.revision.get();

                if !open || !browser.0.preferences.state().with(|p| p.terminal_enabled()) {
                    return;
                }
                let Some(target_dir) = browser.active_local_directory() else {
                    return;
                };
                let Some(active_id) = active_id else {
                    return;
                };
                let Some(entry) = browser.0.terminals.find_session(active_id) else {
                    return;
                };
                if !entry.terminal.is_running() {
                    return;
                }

                let already_synced = {
                    let mut synced = entry.last_synced_dir.borrow_mut();
                    if synced.as_ref() == Some(&target_dir) {
                        true
                    } else {
                        *synced = Some(target_dir.clone());
                        false
                    }
                };
                if already_synced {
                    return;
                }

                let term = entry.terminal.clone();
                browser.0.owner.with(|| {
                    drop(spawn_local(async move {
                        let _ = term.change_directory(target_dir).await;
                    }));
                });
            });
            on_cleanup_local(move || drop(fx));
        });
    }

    pub(super) fn terminal_workspace_resized(&self, width: f32, available: f32) {
        self.0
            .terminals
            .inner
            .workspace_size
            .set((width.max(320.0), available.max(MIN_TERMINAL_HEIGHT)));
        if let Some(content_height) = self.0.preferences.snapshot().files.terminal_content_height {
            if available > MIN_CONTENT_HEIGHT + MIN_TERMINAL_HEIGHT {
                let target = self
                    .0
                    .terminals
                    .clamped_height((available - content_height).max(MIN_TERMINAL_HEIGHT));
                self.0.terminals.inner.height.set(target);
            }
        } else {
            let clamped = self
                .0
                .terminals
                .clamped_height(self.0.terminals.inner.height.get_untracked());
            self.0.terminals.inner.height.set(clamped);
        }
    }

    pub(super) fn terminal_view(&self, presentation: SidebarPresentation) -> AnyView {
        install_terminals_stylesheet();

        let browser = self.clone();
        let open_sig = self.0.terminal;
        let dock = self.0.terminals.clone();
        let active_sig = dock.active;
        let rev_sig = dock.inner.revision;
        let dragging_sig = dock.inner.dragging;
        let maximized_sig = dock.inner.maximized;

        let dock_for_height = dock.clone();
        let dock_for_drag = dock.clone();
        let browser_for_commit = browser.clone();
        let browser_for_key_resize = browser.clone();
        let dock_for_key_resize = dock.clone();

        let rail_width = presentation.width().max(RAIL_MIN_WIDTH);

        let on_handle_pointer_down =
            move |ev: &mut zgui::view::EventCx<'_, zgui::view::events::PointerDown>| {
                ev.capture_pointer();
                ev.stop_propagation();
                ev.prevent_default();
                dragging_sig.set(Some((
                    ev.position.y.0,
                    dock_for_drag.inner.height.get_untracked(),
                )));
            };

        let dock_for_move = dock.clone();
        let on_handle_pointer_move =
            move |ev: &mut zgui::view::EventCx<'_, zgui::view::events::PointerMove>| {
                if let Some((start_y, start_h)) = dragging_sig.get_untracked() {
                    let delta = start_y - ev.position.y.0;
                    let next = dock_for_move.clamped_height(start_h + delta);
                    dock_for_move.inner.height.set(next);
                }
            };

        let dock_for_up = dock.clone();
        let finish_resize = move |browser: &Browser, dock: &Dock| {
            if dragging_sig.get_untracked().is_some() {
                dragging_sig.set(None);
                let available = dock.inner.workspace_size.get_untracked().1;
                let h = dock.inner.height.get_untracked();
                let content_h = (available - h).max(MIN_CONTENT_HEIGHT);
                browser.edit_preferences(|prefs| {
                    prefs.files.terminal_content_height = Some(content_h);
                });
            }
        };

        let b_disp = self.clone();
        let b_height = self.clone();
        let dock_list = dock.clone();
        let dock_pane = dock;
        let b_list = self.clone();
        let b_pane = self.clone();

        let b_new = self.clone();
        let b_cwd = self.clone();
        let cwd_disabled = Signal::derive_local({
            let b = self.clone();
            move || {
                b.track();
                b.active_local_directory().is_none()
            }
        });

        AnyView::new(view! {
            box(
                class = "gds-files-terminal-dock",
                style:display = move || {
                    if open_sig.get()
                        && b_disp.0.preferences.state().with(|p| p.terminal_enabled())
                    {
                        Some("flex".to_owned())
                    } else {
                        Some("none".to_owned())
                    }
                },
                style:height = move || {
                    if open_sig.get()
                        && b_height.0.preferences.state().with(|p| p.terminal_enabled())
                    {
                        Some(format!("{:.1}px", dock_for_height.effective_height()))
                    } else {
                        Some("0px".to_owned())
                    }
                },
            ) {
                box(
                    class = "gds-files-terminal-handle",
                    tabindex = Focus::Sequential,
                    a11y:role = Role::Slider,
                    a11y:label = i18n::text("files-terminal-resize"),
                    attr:data-dragging = move || dragging_sig.get().is_some().then(|| "true".to_owned()),
                    on:pointer_down = on_handle_pointer_down,
                    on:pointer_move = on_handle_pointer_move,
                    on:pointer_up = move |ev| {
                        ev.release_pointer();
                        finish_resize(&browser_for_commit, &dock_for_up);
                    },
                    on:key_down = move |ev| {
                        let step = match &ev.key {
                            Key::Named(NamedKey::ArrowUp) => Some(16.0),
                            Key::Named(NamedKey::ArrowDown) => Some(-16.0),
                            _ => None,
                        };
                        if let Some(delta) = step {
                            let next = dock_for_key_resize.clamped_height(
                                dock_for_key_resize.inner.height.get_untracked() + delta,
                            );
                            dock_for_key_resize.inner.height.set(next);
                            let available = dock_for_key_resize.inner.workspace_size.get_untracked().1;
                            let content_h = (available - next).max(MIN_CONTENT_HEIGHT);
                            browser_for_key_resize.edit_preferences(|prefs| {
                                prefs.files.terminal_content_height = Some(content_h);
                            });
                            ev.prevent_default();
                            ev.stop_propagation();
                        }
                    },
                ) {
                    box(class = "gds-files-terminal-handle__line") {}
                }

                row(class = "gds-files-terminal-body") {
                    column(
                        class = "gds-files-terminal-rail",
                        style:width = format!("{rail_width:.1}px"),
                    ) {
                        row(class = "gds-files-terminal-rail__header") {
                            text(class = "gds-files-terminal-rail__title") {{i18n::text("help-terminal")}}
                            row(class = "gds-files-terminal-rail__actions") {
                                IconButton(
                                    icon = IconName::Plus,
                                    label = i18n::text("files-terminal-new"),
                                    on:click = move |_| b_new.new_terminal(),
                                )
                                IconButton(
                                    icon = IconName::FolderOpen,
                                    label = i18n::text("files-terminal-switch-directory"),
                                    disabled = cwd_disabled,
                                    on:click = move |_| {
                                        let location = b_cwd
                                            .0
                                            .state
                                            .borrow()
                                            .tab()
                                            .navigation
                                            .location()
                                            .clone();
                                        if let Some(dir) = b_cwd
                                            .resolve_path_untracked(&location)
                                            .filter(|path| path.is_dir())
                                        {
                                            b_cwd.new_terminal_at(dir);
                                        }
                                    },
                                )
                            }
                        }
                        column(class = "gds-files-terminal-rail__list") {
                            {move || {
                                let _ = rev_sig.get();
                                let sessions = dock_list.session_snapshot();
                                sessions
                                    .into_iter()
                                    .map(|entry| {
                                        let id = entry.id;
                                        let selected = Signal::derive_local(move || active_sig.get() == Some(id));
                                        let b_select = b_list.clone();
                                        let b_close = b_list.clone();
                                        let b_rename = b_list.clone();
                                        let b_key = b_list.clone();
                                        let entry_for_title = entry.clone();
                                        let entry_for_term = entry.clone();
                                        let renaming_sig = entry.renaming;
                                        let draft_sig = entry.rename_draft;
                                        let rename_ref = entry.rename_ref;
                                        let started_tip = t!(
                                            "files-terminal-started-in",
                                            path = entry.started_in.display().to_string()
                                        );

                                        AnyView::new(view! {
                                            NavItem(
                                                selected = selected,
                                                on:click = move |_| {
                                                    b_select.0.terminals.active.set(Some(id));
                                                    entry_for_term.terminal.focus();
                                                },
                                                on:key_down = move |ev| {
                                                    if ev.key == Key::Named(NamedKey::F2) && ev.modifiers.is_empty() {
                                                        b_key.begin_terminal_rename(id);
                                                        ev.prevent_default();
                                                        ev.stop_propagation();
                                                    }
                                                },
                                            ) {
                                                NavItemContent {
                                                    NavItemHeader {
                                                        Icon(name = IconName::Terminal) {}
                                                        {move || {
                                                            if renaming_sig.get() {
                                                                let b_enter = b_rename.clone();
                                                                let b_esc = b_rename.clone();
                                                                AnyView::new(view! {
                                                                    Input(
                                                                        node_ref = rename_ref,
                                                                        value = Binding::controlled(draft_sig, move |next| {
                                                                            draft_sig.set(next);
                                                                        }),
                                                                        on:click = move |ev| ev.stop_propagation(),
                                                                        on:key_down = move |ev| {
                                                                            match &ev.key {
                                                                                Key::Named(NamedKey::Enter) if ev.modifiers.is_empty() => {
                                                                                    b_enter.commit_terminal_rename(id, true);
                                                                                    ev.prevent_default();
                                                                                    ev.stop_propagation();
                                                                                }
                                                                                Key::Named(NamedKey::Escape) => {
                                                                                    b_esc.commit_terminal_rename(id, false);
                                                                                    ev.prevent_default();
                                                                                    ev.stop_propagation();
                                                                                }
                                                                                _ => {}
                                                                            }
                                                                        },
                                                                    )
                                                                })
                                                            } else {
                                                                let title_entry = entry_for_title.clone();
                                                                AnyView::new(view! {
                                                                    HintTooltip(text = started_tip.clone()) {
                                                                        NavItemLabel {
                                                                            {move || title_entry.display_title()}
                                                                        }
                                                                    }
                                                                })
                                                            }
                                                        }}
                                                    }
                                                    NavItemDetails {{entry.started_in.display().to_string()}}
                                                }
                                                NavItemActions {
                                                    IconButton(
                                                        icon = IconName::Close,
                                                        label = i18n::text("files-terminal-end"),
                                                        on:click = move |ev| {
                                                            ev.stop_propagation();
                                                            b_close.close_terminal_session(id);
                                                        },
                                                    )
                                                }
                                            }
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            }}
                        }
                    }

                    column(class = "gds-files-terminal-pane") {
                        {move || {
                            let _ = rev_sig.get();
                            let active_entry = active_sig
                                .get()
                                .and_then(|id| dock_pane.find_session(id));
                            let Some(entry) = active_entry else {
                                let b_empty = b_pane.clone();
                                let b_hide = b_pane.clone();
                                return AnyView::new(view! {
                                    column(style:flex = "1 1 0%", style:min-height = "0") {
                                        row(class = "gds-files-terminal-pane__header") {
                                            box(class = "gds-files-terminal-pane__meta") {}
                                            row(class = "gds-files-terminal-pane__controls") {
                                                IconButton(
                                                    icon = IconName::Close,
                                                    label = i18n::text("files-terminal-hide"),
                                                    on:click = move |_| b_hide.toggle_terminal(),
                                                )
                                            }
                                        }
                                        column(class = "gds-files-terminal-empty") {
                                            text(class = "gds-files-terminal-empty__text") {{i18n::text("files-terminal-empty")}}
                                            Button(
                                                variant = ButtonVariant::Outline,
                                                size = ButtonSize::Sm,
                                                on:click = move |_| b_empty.new_terminal(),
                                            ) {
                                                Icon(name = IconName::Plus) {}
                                                text {{i18n::text("files-terminal-new")}}
                                            }
                                        }
                                    }
                                });
                            };

                            let term = entry.terminal.clone();
                            let id = entry.id;
                            let cwd_text = term.cwd().display().to_string();
                            let error_sig = term.error();
                            let status_sig = term.status();
                            let status_term = term.clone();

                            let b_rename = b_pane.clone();
                            let b_close = b_pane.clone();
                            let b_hide = b_pane.clone();
                            let b_keys = b_pane.clone();

                            let max_icon = Signal::derive_local(move || {
                                if maximized_sig.get() {
                                    IconName::Minimize2
                                } else {
                                    IconName::Maximize2
                                }
                            });
                            let max_label = Signal::derive_local(move || {
                                if maximized_sig.get() {
                                    i18n::text("files-terminal-restore")
                                } else {
                                    i18n::text("files-terminal-maximize")
                                }
                            });

                            let on_key = UnsyncCallback::new(move |(key, modifiers): (Key, Modifiers)| -> bool {
                                let key_event = KeyEvent {
                                    key: key.clone(),
                                    key_without_modifiers: key.clone(),
                                    physical: PhysicalKey::Unidentified(0),
                                    location: KeyLocation::Standard,
                                    repeat: false,
                                };
                                if Command::Terminal.matches(&key_event, modifiers) {
                                    b_keys.toggle_terminal();
                                    return true;
                                }
                                if key == Key::Named(NamedKey::F6) && modifiers.is_empty() {
                                    b_keys.focus_content();
                                    return true;
                                }
                                if key == Key::Named(NamedKey::F2) && modifiers.is_empty() {
                                    b_keys.begin_terminal_rename(id);
                                    return true;
                                }
                                let ctrl_shift = modifiers.contains(Modifiers::CONTROL | Modifiers::SHIFT)
                                    && !modifiers.contains(Modifiers::ALT)
                                    && !modifiers.contains(Modifiers::META);
                                if ctrl_shift && let Key::Character(ch) = &key {
                                    if ch.eq_ignore_ascii_case("n") {
                                        b_keys.new_terminal();
                                        return true;
                                    }
                                    if ch.eq_ignore_ascii_case("w") {
                                        b_keys.close_terminal_session(id);
                                        return true;
                                    }
                                }
                                false
                            });

                            AnyView::new(view! {
                                column(style:flex = "1 1 0%", style:min-height = "0") {
                                    row(class = "gds-files-terminal-pane__header") {
                                        row(class = "gds-files-terminal-pane__meta") {
                                            text(class = "gds-files-terminal-pane__cwd") {{cwd_text}}
                                            text(
                                                class = "gds-files-terminal-pane__status",
                                                attr:data-error = move || error_sig.get().is_some().then(|| "true".to_owned()),
                                            ) {{move || {
                                                let _ = status_sig.get();
                                                error_sig
                                                    .get()
                                                    .unwrap_or_else(|| status_term.status_label())
                                            }}}
                                        }
                                        row(class = "gds-files-terminal-pane__controls") {
                                            IconButton(
                                                icon = IconName::Pencil,
                                                label = i18n::text("files-terminal-rename"),
                                                on:click = move |_| b_rename.begin_terminal_rename(id),
                                            )
                                            IconButton(
                                                icon = max_icon,
                                                label = max_label,
                                                on:click = move |_| {
                                                    maximized_sig.update(|m| *m = !*m);
                                                },
                                            )
                                            IconButton(
                                                icon = IconName::Close,
                                                label = i18n::text("files-terminal-end"),
                                                on:click = move |_| b_close.close_terminal_session(id),
                                            )
                                            IconButton(
                                                icon = IconName::ChevronDown,
                                                label = i18n::text("files-terminal-hide"),
                                                on:click = move |_| b_hide.toggle_terminal(),
                                            )
                                        }
                                    }
                                    TerminalView(
                                        terminal = term,
                                        on_key = on_key,
                                        node_ref = entry.view_ref,
                                        class = "gds-files-terminal-pane__surface",
                                    )
                                }
                            })
                        }}
                    }
                }
            }
        })
    }
}
