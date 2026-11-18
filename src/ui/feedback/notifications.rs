//! Suite-wide expanded toast notifications emitting `Role::Alert` (AT-SPI `'notification'`).

mod interaction;
pub mod surface;
#[cfg(test)]
mod tests;

use std::cell::RefCell;

pub use interaction::{close_all, close_current, focus, is_focused};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, provide_local_context, use_local_context};
use zgui::{component, view};

pub const MAX_CONTENT_HEIGHT: f32 = 240.;

/// Severity level for a user-visible notification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Level {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

/// Structured notification payload with summary and optional expandable items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub level: Level,
    pub summary: String,
    pub details: Vec<String>,
}

impl Report {
    #[must_use]
    pub fn new(level: Level, summary: impl Into<String>) -> Self {
        Self {
            level,
            summary: summary.into(),
            details: Vec::new(),
        }
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.details.push(detail.into());
        self
    }

    #[must_use]
    pub fn details(mut self, details: Vec<String>) -> Self {
        self.details.extend(details);
        self
    }
}

/// Queued notification entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub id: u64,
    pub report: Report,
    pub autohide: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct NotificationQueue {
    items: RwSignal<Vec<Notification>, LocalStorage>,
    next_id: RwSignal<u64, LocalStorage>,
    focused: RwSignal<bool, LocalStorage>,
}

thread_local! {
    static FALLBACK_QUEUE: RefCell<Option<NotificationQueue>> = const { RefCell::new(None) };
}

impl NotificationQueue {
    fn new() -> Self {
        Self {
            items: RwSignal::new_local(Vec::new()),
            next_id: RwSignal::new_local(1),
            focused: RwSignal::new_local(false),
        }
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

    pub(crate) fn push(self, report: Report) {
        let id = self.next_id.get_untracked();
        self.next_id.set(id.saturating_add(1));
        self.items.update(|items| {
            items.insert(
                0,
                Notification {
                    id,
                    report,
                    autohide: true,
                },
            );
        });
    }

    pub(crate) fn items(self) -> RwSignal<Vec<Notification>, LocalStorage> {
        self.items
    }

    pub(crate) fn focused(self) -> RwSignal<bool, LocalStorage> {
        self.focused
    }
}

/// Mounts the per-window notification portal stack (`Role::Alert` -> AT-SPI `'notification'`).
#[component]
pub fn NotificationHost(children: Children) -> impl IntoView {
    let queue = NotificationQueue::new();
    provide_local_context(queue);
    let items = queue.items();
    view! {
        box(class = "gds-notification-root") {
            {children.into_view_once()}
            Portal(layer = OverlayLayer::Toast) {
                column(class = "gds-notification-column") {
                    for entry in move || items.get(), key = |n| n.id {
                        {surface::render_notification(entry)}
                    }
                }
            }
        }
    }
}

pub fn report(report: Report) {
    NotificationQueue::active().push(report);
}

pub fn info(message: impl Into<String>) {
    report(Report::new(Level::Info, message));
}

pub fn success(message: impl Into<String>) {
    report(Report::new(Level::Success, message));
}

pub fn warning(message: impl Into<String>) {
    report(Report::new(Level::Warning, message));
}

pub fn error(message: impl Into<String>) {
    report(Report::new(Level::Error, message));
}
