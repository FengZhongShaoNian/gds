use zgui::prelude::*;
use zgui::view;

use super::Notification;

/// Renders one notification card with `Role::Alert` (AT-SPI `'notification'`).
pub(super) fn render_notification(entry: Notification) -> AnyView {
    let summary = entry.report.summary;
    AnyView::new(view! {
        box(
            class = "gds-notification-card",
            a11y:role = Role::Alert,
            a11y:label = summary.clone()
        ) {
            text {{summary}}
        }
    })
}
