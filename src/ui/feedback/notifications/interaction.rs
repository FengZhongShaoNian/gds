use zgui::prelude::*;
use zgui::runtime::try_use_clipboard;

use super::NotificationQueue;

/// Moves keyboard focus into the notification stack and pauses auto-dismiss timers while focused.
pub fn focus() {
    let queue = NotificationQueue::active();
    let items = queue.items().get_untracked();
    if items.is_empty() {
        return;
    }
    queue.capture_previous_focus();
    let target_id = queue
        .selected_id()
        .get_untracked()
        .filter(|id| items.iter().any(|item| item.id == *id))
        .unwrap_or(items[0].id);
    queue.focus_notification(target_id);
}

/// Dismisses the currently focused notification, or the newest notification when unfocused.
pub fn close_current() {
    let queue = NotificationQueue::active();
    let items = queue.items().get_untracked();
    let target = queue
        .selected_id()
        .get_untracked()
        .filter(|id| items.iter().any(|item| item.id == *id))
        .or_else(|| items.first().map(|item| item.id));
    if let Some(id) = target {
        queue.dismiss(id);
    }
}

/// Dismisses every active notification and restores any captured prior window focus.
pub fn close_all() {
    NotificationQueue::active().dismiss_all();
}

/// Returns `true` when keyboard focus currently sits inside the notification stack.
#[must_use]
pub fn is_focused() -> bool {
    let focused = NotificationQueue::active().focused();
    focused.track();
    focused.get_untracked()
}

pub(crate) fn select_next() {
    move_selection(1);
}

pub(crate) fn select_prev() {
    move_selection(-1);
}

pub(crate) fn select_first() {
    let queue = NotificationQueue::active();
    if let Some(first) = queue.items().get_untracked().first() {
        queue.focus_notification(first.id);
    }
}

pub(crate) fn select_last() {
    let queue = NotificationQueue::active();
    if let Some(last) = queue.items().get_untracked().last() {
        queue.focus_notification(last.id);
    }
}

pub(crate) fn copy_selected() {
    let queue = NotificationQueue::active();
    let items = queue.items().get_untracked();
    let target = queue
        .selected_id()
        .get_untracked()
        .and_then(|id| items.iter().find(|item| item.id == id))
        .or_else(|| items.first());
    if let Some(entry) = target
        && let Some(clipboard) = try_use_clipboard()
    {
        clipboard.set_text(ClipboardKind::Standard, entry.report.copy_text());
    }
}

fn move_selection(delta: isize) {
    let queue = NotificationQueue::active();
    let items = queue.items().get_untracked();
    if items.is_empty() {
        return;
    }
    let current_index = queue
        .selected_id()
        .get_untracked()
        .and_then(|id| items.iter().position(|item| item.id == id))
        .unwrap_or(0);
    let len = items.len();
    let next_index = if delta >= 0 {
        (current_index + 1) % len
    } else if current_index == 0 {
        len - 1
    } else {
        current_index - 1
    };
    queue.focus_notification(items[next_index].id);
}
