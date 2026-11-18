//! Suite-wide expanded toast notifications emitting `Role::Alert` (AT-SPI `'notification'`).

mod interaction;
pub mod surface;
#[cfg(test)]
mod tests;

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::time::Duration;

pub use interaction::{close_all, close_current, focus, is_focused};
use zgui::prelude::*;
use zgui::reactive::{
    LocalStorage, RenderEffect, RwSignal, on_cleanup_local, provide_local_context,
    use_local_context,
};
use zgui::view::{
    ListenerGuard, OverlayLayer, PortalProps, ShowProps, TimeoutHandle, current_host, events,
    set_timeout,
};
use zgui::vocab::{Key, Live, Modifiers, NamedKey};
use zgui::{component, css, view};

use crate::app::Gds;
use crate::appearance::tokens::{NOTIFICATION_INSET, NOTIFICATION_TOP_GAP, TOOLBAR_HEIGHT};
use crate::ui::actions::{ActionDispatcher, Command};

pub const MAX_CONTENT_HEIGHT: f32 = 240.;
pub const DETAIL_LIMIT: usize = 4;
pub const AUTO_DISMISS_DELAY: Duration = Duration::from_secs(5);

const HOST_SHEET: &str = css!(
    r#"
    .gds-notification-root {
        display: contents;
    }

    .gds-notification-column {
        position: fixed;
        display: flex;
        flex-direction: column;
        gap: 8px;
        width: 360px;
        max-width: calc(100vw - 24px);
        pointer-events: auto;
    }
    "#
);

/// Severity level for a user-visible notification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Level {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl Level {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    #[must_use]
    pub const fn a11y_live(self) -> Live {
        match self {
            Self::Error => Live::Assertive,
            Self::Info | Self::Success | Self::Warning => Live::Polite,
        }
    }

    #[must_use]
    pub const fn default_autohide(self) -> bool {
        matches!(self, Self::Info | Self::Success)
    }
}

/// Structured notification payload with summary and optional expandable items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub level: Level,
    pub summary: String,
    pub details: Vec<String>,
    slot: Option<TypeId>,
    autohide_override: Option<bool>,
}

impl Report {
    /// Creates a report with `level` and primary `summary` text.
    #[must_use]
    pub fn new(level: Level, summary: impl Into<String>) -> Self {
        Self {
            level,
            summary: summary.into(),
            details: Vec::new(),
            slot: None,
            autohide_override: None,
        }
    }

    /// Appends one detail line.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.details.push(detail.into());
        self
    }

    /// Appends multiple detail lines.
    #[must_use]
    pub fn details(mut self, details: Vec<String>) -> Self {
        self.details.extend(details);
        self
    }

    /// Marks this report as replacing any live notification tagged with slot type `T`.
    #[must_use]
    pub fn replacing<T: 'static>(mut self) -> Self {
        self.slot = Some(TypeId::of::<T>());
        self
    }

    /// Overrides whether the notification automatically dismisses after [`AUTO_DISMISS_DELAY`].
    #[must_use]
    pub fn autohide(mut self, autohide: bool) -> Self {
        self.autohide_override = Some(autohide);
        self
    }

    /// Publishes this report to the active window's notification stack.
    pub fn show(self) {
        report(self);
    }

    /// Formats summary and detail lines as plain text for clipboard copying.
    #[must_use]
    pub fn copy_text(&self) -> String {
        if self.details.is_empty() {
            return self.summary.clone();
        }
        let mut out = self.summary.clone();
        for detail in &self.details {
            out.push('\n');
            out.push_str(detail);
        }
        out
    }
}

/// Convenience builder summarizing multi-item file or background operations.
#[derive(Clone, Debug, Default)]
pub struct Operation {
    summary: String,
    details: Vec<String>,
    level: Level,
    slot: Option<TypeId>,
}

impl Operation {
    /// Starts a batch operation report summary.
    #[must_use]
    pub fn new(level: Level, summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            details: Vec::new(),
            level,
            slot: None,
        }
    }

    /// Adds an item line to the operation report.
    #[must_use]
    pub fn item(mut self, line: impl Into<String>) -> Self {
        self.details.push(line.into());
        self
    }

    /// Tags this operation report with a replacement slot type `T`.
    #[must_use]
    pub fn replacing<T: 'static>(mut self) -> Self {
        self.slot = Some(TypeId::of::<T>());
        self
    }

    /// Converts this operation summary into a [`Report`], appending a localized overflow count when
    /// details exceed [`DETAIL_LIMIT`].
    #[must_use]
    pub fn into_report(self) -> Report {
        let mut report = Report::new(self.level, self.summary).details(self.details);
        report.slot = self.slot;
        report
    }
}

/// Formats the localized `"And N more items"` overflow label for truncated detail lists.
#[must_use]
pub(crate) fn more_items_label(remaining: usize) -> String {
    Gds::try_current().map_or_else(
        || format!("And {remaining} more items"),
        |gds| gds.i18n().count("operation-more-items", remaining),
    )
}

/// Queued notification entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub id: u64,
    pub report: Report,
    pub autohide: bool,
}

#[derive(Default)]
struct QueueRuntime {
    timers: HashMap<u64, TimeoutHandle>,
    slot_ids: HashMap<TypeId, u64>,
    card_refs: HashMap<u64, NodeRef>,
    host_ref: Option<NodeRef>,
    previous_focus: Option<NodeId>,
}

#[derive(Clone, Copy)]
pub(crate) struct NotificationQueue {
    items: RwSignal<Vec<Notification>, LocalStorage>,
    next_id: RwSignal<u64, LocalStorage>,
    focused: RwSignal<bool, LocalStorage>,
    selected_id: RwSignal<Option<u64>, LocalStorage>,
    expanded_ids: RwSignal<BTreeSet<u64>, LocalStorage>,
    hovered_ids: RwSignal<BTreeSet<u64>, LocalStorage>,
    runtime: RwSignal<Option<Rc<RefCell<QueueRuntime>>>, LocalStorage>,
}

thread_local! {
    static FALLBACK_QUEUE: RefCell<Option<NotificationQueue>> = const { RefCell::new(None) };
}

impl NotificationQueue {
    fn new() -> Self {
        let rt = Rc::new(RefCell::new(QueueRuntime::default()));
        Self {
            items: RwSignal::new_local(Vec::new()),
            next_id: RwSignal::new_local(1),
            focused: RwSignal::new_local(false),
            selected_id: RwSignal::new_local(None),
            expanded_ids: RwSignal::new_local(BTreeSet::new()),
            hovered_ids: RwSignal::new_local(BTreeSet::new()),
            runtime: RwSignal::new_local(Some(rt)),
        }
    }

    fn runtime_rc(self) -> Option<Rc<RefCell<QueueRuntime>>> {
        self.runtime.try_get_untracked().flatten()
    }

    pub(crate) fn active() -> Self {
        if let Some(queue) = use_local_context::<Self>() {
            return queue;
        }
        FALLBACK_QUEUE.with(|slot| {
            if let Some(queue) = *slot.borrow() {
                return queue;
            }
            let queue = Self::new();
            *slot.borrow_mut() = Some(queue);
            queue
        })
    }

    fn register_host_ref(self, node_ref: NodeRef) {
        if let Some(rt) = self.runtime_rc() {
            rt.borrow_mut().host_ref = Some(node_ref);
        }
    }

    pub(crate) fn register_card_ref(self, id: u64, node_ref: NodeRef) {
        if let Some(rt) = self.runtime_rc() {
            rt.borrow_mut().card_refs.insert(id, node_ref);
        }
    }

    pub(crate) fn push(self, report: Report) {
        let slot = report.slot;
        let autohide = report
            .autohide_override
            .unwrap_or_else(|| report.level.default_autohide());

        let replaced_id = slot.and_then(|slot_key| {
            self.runtime_rc()
                .and_then(|rt| rt.borrow().slot_ids.get(&slot_key).copied())
        });

        let id = if let Some(existing_id) = replaced_id
            && self
                .items
                .get_untracked()
                .iter()
                .any(|item| item.id == existing_id)
        {
            self.cancel_timer(existing_id);
            self.items.update(|items| {
                if let Some(entry) = items.iter_mut().find(|item| item.id == existing_id) {
                    entry.report = report;
                    entry.autohide = autohide;
                }
            });
            existing_id
        } else {
            let id = self.next_id.get_untracked();
            self.next_id.set(id.saturating_add(1));
            self.items.update(|items| {
                items.insert(
                    0,
                    Notification {
                        id,
                        report,
                        autohide,
                    },
                );
            });
            if let Some(slot_key) = slot
                && let Some(rt) = self.runtime_rc()
            {
                rt.borrow_mut().slot_ids.insert(slot_key, id);
            }
            id
        };

        if self.focused.get_untracked() && self.selected_id.get_untracked().is_none() {
            self.selected_id.set(Some(id));
        }
        self.schedule_autohide_if_eligible(id);
    }

    pub(crate) fn dismiss(self, id: u64) {
        self.cancel_timer(id);

        let mut next_focus_target: Option<(u64, NodeRef)> = None;
        let mut restore_node: Option<NodeId> = None;

        self.items.update(|items| {
            items.retain(|item| item.id != id);
        });
        self.expanded_ids.update(|set| {
            set.remove(&id);
        });
        self.hovered_ids.update(|set| {
            set.remove(&id);
        });

        let remaining = self.items.get_untracked();
        if self.selected_id.get_untracked() == Some(id) {
            let next_id = remaining.first().map(|item| item.id);
            self.selected_id.set(next_id);
        }

        if let Some(rt) = self.runtime_rc() {
            let mut rt_mut = rt.borrow_mut();
            rt_mut.card_refs.remove(&id);
            rt_mut.slot_ids.retain(|_, existing| *existing != id);
            if remaining.is_empty() {
                restore_node = rt_mut.previous_focus.take();
            } else if self.focused.get_untracked()
                && let Some(target_id) = self.selected_id.get_untracked()
                && let Some(node_ref) = rt_mut.card_refs.get(&target_id)
            {
                next_focus_target = Some((target_id, *node_ref));
            }
        }

        if remaining.is_empty() {
            let was_focused = self.focused.get_untracked();
            self.focused.set(false);
            if was_focused
                && let Some(prev) = restore_node
                && let Some(host) = current_host()
            {
                host.focus(prev);
            }
        } else if let Some((target_id, node_ref)) = next_focus_target {
            self.selected_id.set(Some(target_id));
            node_ref.focus();
        }
    }

    pub(crate) fn dismiss_all(self) {
        let mut drained_timers = Vec::new();
        let mut restore_node = None;

        if let Some(rt) = self.runtime_rc() {
            let mut rt_mut = rt.borrow_mut();
            drained_timers.extend(rt_mut.timers.drain().map(|(_, handle)| handle));
            rt_mut.card_refs.clear();
            rt_mut.slot_ids.clear();
            restore_node = rt_mut.previous_focus.take();
        }
        drop(drained_timers);

        let was_focused = self.focused.get_untracked();
        self.items.update(Vec::clear);
        self.expanded_ids.update(BTreeSet::clear);
        self.hovered_ids.update(BTreeSet::clear);
        self.selected_id.set(None);
        self.focused.set(false);

        if was_focused
            && let Some(prev) = restore_node
            && let Some(host) = current_host()
        {
            host.focus(prev);
        }
    }

    pub(crate) fn items(self) -> RwSignal<Vec<Notification>, LocalStorage> {
        self.items
    }

    pub(crate) fn focused(self) -> RwSignal<bool, LocalStorage> {
        self.focused
    }

    pub(crate) fn selected_id(self) -> RwSignal<Option<u64>, LocalStorage> {
        self.selected_id
    }

    pub(crate) fn is_expanded(self, id: u64) -> bool {
        self.expanded_ids.get().contains(&id)
    }

    pub(crate) fn toggle_expanded(self, id: u64) {
        self.expanded_ids.update(|set| {
            if !set.remove(&id) {
                set.insert(id);
            }
        });
    }

    pub(crate) fn set_hovered(self, id: u64, hovered: bool) {
        self.hovered_ids.update(|set| {
            if hovered {
                set.insert(id);
            } else {
                set.remove(&id);
            }
        });
        if hovered {
            self.cancel_timer(id);
        } else {
            self.schedule_autohide_if_eligible(id);
        }
    }

    pub(crate) fn mark_card_focused(self, id: u64) {
        self.capture_previous_focus();
        self.selected_id.set(Some(id));
        self.focused.set(true);
        self.pause_all_timers();
    }

    pub(crate) fn clear_focus(self, restore_focus: bool) {
        let mut restore_node = None;
        if let Some(rt) = self.runtime_rc() {
            restore_node = rt.borrow_mut().previous_focus.take();
        }
        self.focused.set(false);
        self.resume_unhovered_timers();
        if restore_focus
            && let Some(prev) = restore_node
            && let Some(host) = current_host()
        {
            host.focus(prev);
        }
    }

    pub(crate) fn focus_notification(self, id: u64) {
        self.selected_id.set(Some(id));
        self.focused.set(true);
        self.pause_all_timers();
        let card = self
            .runtime_rc()
            .and_then(|rt| rt.borrow().card_refs.get(&id).cloned());
        if let Some(card_ref) = card {
            card_ref.focus();
        }
    }

    pub(crate) fn capture_previous_focus(self) {
        let Some(host) = current_host() else {
            return;
        };
        let Some(active) = host.focused().get_untracked() else {
            return;
        };
        let Some(rt) = self.runtime_rc() else {
            return;
        };
        let mut rt_mut = rt.borrow_mut();
        let inside_notifications = rt_mut
            .host_ref
            .as_ref()
            .is_some_and(|r| r.get_untracked() == Some(active) || r.contains(active));
        if !inside_notifications && rt_mut.previous_focus.is_none() {
            rt_mut.previous_focus = Some(active);
        }
    }

    fn cancel_timer(self, id: u64) {
        let removed = self
            .runtime_rc()
            .and_then(|rt| rt.borrow_mut().timers.remove(&id));
        drop(removed);
    }

    fn pause_all_timers(self) {
        let mut drained = Vec::new();
        if let Some(rt) = self.runtime_rc() {
            drained.extend(rt.borrow_mut().timers.drain().map(|(_, handle)| handle));
        }
        drop(drained);
    }

    fn resume_unhovered_timers(self) {
        let candidates: Vec<u64> = self
            .items
            .get_untracked()
            .into_iter()
            .filter(|item| item.autohide)
            .map(|item| item.id)
            .collect();
        for id in candidates {
            self.schedule_autohide_if_eligible(id);
        }
    }

    fn schedule_autohide_if_eligible(self, id: u64) {
        if current_host().is_none() {
            return;
        }
        if self.focused.get_untracked() || self.hovered_ids.get_untracked().contains(&id) {
            return;
        }
        let should_autohide = self
            .items
            .get_untracked()
            .into_iter()
            .find(|item| item.id == id)
            .is_some_and(|item| item.autohide);
        if !should_autohide {
            return;
        }

        self.cancel_timer(id);
        let handle = set_timeout(AUTO_DISMISS_DELAY, move || {
            if self.focused.get_untracked() || self.hovered_ids.get_untracked().contains(&id) {
                return;
            }
            self.dismiss(id);
        });
        if let Some(rt) = self.runtime_rc() {
            let old = rt.borrow_mut().timers.insert(id, handle);
            drop(old);
        }
    }

    fn clear_timers(self) {
        let mut drained = Vec::new();
        if let Some(rt) = self.runtime_rc() {
            drained.extend(rt.borrow_mut().timers.drain().map(|(_, handle)| handle));
        }
        drop(drained);
    }
}

fn handle_window_shortcut(queue: NotificationQueue, ev: &mut EventCx<'_, events::KeyDown>) {
    let dispatcher = ActionDispatcher::commands();
    match dispatcher.match_event(ev, ev.modifiers) {
        Some(Command::FocusNotifications) if !queue.items().get_untracked().is_empty() => {
            interaction::focus();
            ev.prevent_default();
            ev.stop_propagation();
        }
        Some(Command::CloseNotification) if !queue.items().get_untracked().is_empty() => {
            interaction::close_current();
            ev.prevent_default();
            ev.stop_propagation();
        }
        Some(Command::CloseAllNotifications) if !queue.items().get_untracked().is_empty() => {
            interaction::close_all();
            ev.prevent_default();
            ev.stop_propagation();
        }
        _ => {}
    }
}

fn handle_stack_key_down(queue: NotificationQueue, ev: &mut EventCx<'_, events::KeyDown>) {
    if ev.repeat {
        return;
    }
    let modifiers = ev.modifiers.held();
    match &ev.key {
        Key::Named(NamedKey::ArrowDown) if modifiers.is_empty() => {
            interaction::select_next();
            ev.prevent_default();
            ev.stop_propagation();
        }
        Key::Named(NamedKey::ArrowUp) if modifiers.is_empty() => {
            interaction::select_prev();
            ev.prevent_default();
            ev.stop_propagation();
        }
        Key::Named(NamedKey::Home) if modifiers.is_empty() => {
            interaction::select_first();
            ev.prevent_default();
            ev.stop_propagation();
        }
        Key::Named(NamedKey::End) if modifiers.is_empty() => {
            interaction::select_last();
            ev.prevent_default();
            ev.stop_propagation();
        }
        Key::Named(NamedKey::Escape) if modifiers.is_empty() => {
            queue.clear_focus(true);
            ev.prevent_default();
            ev.stop_propagation();
        }
        Key::Named(NamedKey::Delete) if modifiers.is_empty() => {
            interaction::close_current();
            ev.prevent_default();
            ev.stop_propagation();
        }
        Key::Character(ch)
            if (modifiers == Modifiers::PRIMARY || modifiers == Modifiers::CONTROL)
                && ch.as_str().eq_ignore_ascii_case("c") =>
        {
            interaction::copy_selected();
            ev.prevent_default();
            ev.stop_propagation();
        }
        _ => {}
    }
}

/// Mounts the per-window notification portal stack (`Role::Alert` -> AT-SPI `'notification'`).
#[component]
pub fn NotificationHost(children: Children) -> impl IntoView {
    if current_host().is_some() {
        zgui::view::install_stylesheet("gds-ui-notifications-host", HOST_SHEET);
        zgui::view::install_stylesheet("gds-ui-notifications-surface", surface::SHEET);
    }

    let queue = NotificationQueue::new();
    provide_local_context(queue);
    let items = queue.items();

    let host_ref = NodeRef::new();
    let column_ref = NodeRef::new();
    queue.register_host_ref(column_ref);

    let root_guard: Rc<RefCell<Option<ListenerGuard>>> = Rc::new(RefCell::new(None));
    let watch = {
        let root_guard = Rc::clone(&root_guard);
        RenderEffect::new(move |_| {
            if host_ref.get().is_none() || root_guard.borrow().is_some() {
                return;
            }
            let Some(root) = host_ref.window_root() else {
                return;
            };
            let guard = root.listen(
                events::KEY_DOWN,
                ListenerOptions::CAPTURE,
                move |ev: &mut EventCx<'_, events::KeyDown>| handle_window_shortcut(queue, ev),
            );
            *root_guard.borrow_mut() = guard;
        })
    };

    if let Some(host) = current_host() {
        let host_focus = host.focused();
        let col_check = column_ref;
        let focus_watch = RenderEffect::new(move |_| {
            if !queue.focused().get_untracked() {
                return;
            }
            let still_inside = host_focus.get().is_some_and(|active| {
                col_check.get_untracked() == Some(active) || col_check.contains(active)
            });
            if !still_inside {
                queue.clear_focus(false);
            }
        });
        on_cleanup_local(move || drop(focus_watch));
    }

    on_cleanup_local(move || {
        drop(watch);
        drop(root_guard.borrow_mut().take());
        queue.clear_timers();
    });

    view! {
        box(node_ref = host_ref, class = "gds-notification-root") {
            {children.into_view_once()}
            Portal(layer = OverlayLayer::Toast) {
                Show(when = move || !items.get().is_empty()) {
                    column(
                        node_ref = column_ref,
                        class = "gds-notification-column",
                        style:top = Some(format!("{}px", TOOLBAR_HEIGHT + NOTIFICATION_TOP_GAP)),
                        style:right = Some(format!("{NOTIFICATION_INSET}px")),
                        on:key_down = move |ev: &mut EventCx<'_, events::KeyDown>| {
                            handle_stack_key_down(queue, ev);
                        },
                        on:focus_out = move |ev: &mut EventCx<'_, events::FocusOut>| {
                            if ev.related.is_none() {
                                queue.clear_focus(false);
                            }
                        }
                    ) {
                        for entry in move || items.get(), key = |n| n.id {
                            {surface::render_notification(queue, entry)}
                        }
                    }
                }
            }
        }
    }
}

/// Pushes a structured [`Report`] into the active notification stack.
pub fn report(report: Report) {
    NotificationQueue::active().push(report);
}

/// Pushes an informational notification.
pub fn info(message: impl Into<String>) {
    report(Report::new(Level::Info, message));
}

/// Pushes a success notification.
pub fn success(message: impl Into<String>) {
    report(Report::new(Level::Success, message));
}

/// Pushes a warning notification.
pub fn warning(message: impl Into<String>) {
    report(Report::new(Level::Warning, message));
}

/// Pushes an error notification.
pub fn error(message: impl Into<String>) {
    report(Report::new(Level::Error, message));
}
