use zgui::prelude::*;

use super::NotificationQueue;

pub fn focus() {
    NotificationQueue::active().focused().set(true);
}

pub fn close_current() {
    let queue = NotificationQueue::active();
    queue.items().update(|items| {
        if !items.is_empty() {
            items.remove(0);
        }
    });
}

pub fn close_all() {
    let queue = NotificationQueue::active();
    queue.items().update(Vec::clear);
    queue.focused().set(false);
}

#[must_use]
pub fn is_focused() -> bool {
    NotificationQueue::active().focused().get()
}
