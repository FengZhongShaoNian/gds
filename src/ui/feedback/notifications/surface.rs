use zgui::prelude::*;
use zgui::runtime::try_use_clipboard;
use zgui::{css, view};
use zgui_ui::{ButtonProps, ButtonSize, ButtonVariant};

use super::{
    DETAIL_LIMIT, Level, MAX_CONTENT_HEIGHT, Notification, NotificationQueue, more_items_label,
};
use crate::app::Gds;
use crate::ui::button::IconButtonProps;
use crate::ui::icon::{IconName, IconProps};

pub(super) const SHEET: &str = css!(
    r#"
    .gds-notification-card {
        display: flex;
        flex-direction: column;
        gap: 6px;
        padding: 10px 12px;
        border-radius: 8px;
        border: 1px solid var(--gds-border);
        border-left-width: 3px;
        background: var(--gds-surface);
        color: var(--gds-foreground);
        box-shadow: 0 6px 20px rgba(0, 0, 0, 0.28);
    }

    .gds-notification-card[data-selected="true"] {
        outline: 2px solid var(--gds-outline);
        outline-offset: 1px;
    }

    .gds-notification-card[data-level="info"] {
        border-left-color: var(--gds-primary);
    }

    .gds-notification-card[data-level="success"] {
        border-left-color: var(--gds-success);
    }

    .gds-notification-card[data-level="warning"] {
        border-left-color: var(--gds-warning);
    }

    .gds-notification-card[data-level="error"] {
        border-left-color: var(--gds-error);
    }

    .gds-notification-header {
        display: flex;
        align-items: center;
        gap: 8px;
    }

    .gds-notification-summary {
        flex: 1;
        min-width: 0;
        font-size: 13px;
        font-weight: 600;
        overflow-wrap: anywhere;
    }

    .gds-notification-actions {
        display: flex;
        align-items: center;
        gap: 2px;
        flex-shrink: 0;
    }

    .gds-notification-details {
        display: flex;
        flex-direction: column;
        gap: 3px;
        padding-left: 22px;
        overflow-y: auto;
    }

    .gds-notification-detail-line {
        font-size: 12px;
        color: var(--gds-muted-foreground);
        overflow-wrap: anywhere;
    }
    "#
);

fn level_icon(level: Level) -> IconName {
    match level {
        Level::Info => IconName::Info,
        Level::Success => IconName::CircleCheck,
        Level::Warning => IconName::TriangleAlert,
        Level::Error => IconName::CircleAlert,
    }
}

/// Renders one notification card with `Role::Alert` (AT-SPI `'notification'`), hover timer pause,
/// detail expansion beyond [`DETAIL_LIMIT`], and clipboard copy.
pub(super) fn render_notification(queue: NotificationQueue, entry: Notification) -> AnyView {
    let id = entry.id;
    let level_attr = entry.report.level.as_str().to_owned();
    let live = entry.report.level.a11y_live();
    let icon = level_icon(entry.report.level);
    let summary = entry.report.summary.clone();
    let copy_payload = entry.report.copy_text();
    let details = entry.report.details.clone();
    let has_details = !details.is_empty();
    let overflow_count = details.len().saturating_sub(DETAIL_LIMIT);

    let copy_label = Gds::try_current().map_or_else(
        || "Copy notification".to_owned(),
        |gds| gds.i18n().text("notification-copy"),
    );
    let close_label = Gds::try_current().map_or_else(
        || "Close current notification".to_owned(),
        |gds| gds.i18n().text("notification-close"),
    );

    let card_ref = NodeRef::new();
    queue.register_card_ref(id, card_ref);

    let max_content_px = format!("{MAX_CONTENT_HEIGHT}px");

    AnyView::new(view! {
        column(
            node_ref = card_ref,
            class = "gds-notification-card",
            tabindex = Focus::Sequential,
            a11y:role = Role::Alert,
            a11y:live = live,
            a11y:label = summary.clone(),
            attr:data-level = level_attr,
            attr:data-selected = move || {
                (queue.focused().get() && queue.selected_id().get() == Some(id))
                    .then(|| "true".to_owned())
            },
            on:pointer_enter = move |_| queue.set_hovered(id, true),
            on:pointer_leave = move |_| queue.set_hovered(id, false),
            on:focus_in = move |_| queue.mark_card_focused(id),
        ) {
            row(class = "gds-notification-header") {
                Icon(name = icon)
                text(class = "gds-notification-summary") {{summary}}
                row(class = "gds-notification-actions") {
                    IconButton(
                        icon = IconName::Copy,
                        label = copy_label,
                        on:click = move |ev: &mut EventCx<'_, zgui::view::events::Click>| {
                            ev.stop_propagation();
                            if let Some(clipboard) = try_use_clipboard() {
                                clipboard.set_text(ClipboardKind::Standard, copy_payload.clone());
                            }
                        }
                    )
                    IconButton(
                        icon = IconName::Close,
                        label = close_label,
                        on:click = move |ev: &mut EventCx<'_, zgui::view::events::Click>| {
                            ev.stop_propagation();
                            queue.dismiss(id);
                        }
                    )
                }
            }
            {has_details.then(|| {
                let details_vec = details.clone();
                view! {
                    column(
                        class = "gds-notification-details",
                        style:max_height = max_content_px.clone(),
                    ) {
                        for entry_line in move || {
                            let limit = if queue.is_expanded(id) {
                                details_vec.len()
                            } else {
                                details_vec.len().min(DETAIL_LIMIT)
                            };
                            details_vec[..limit]
                                .iter()
                                .cloned()
                                .enumerate()
                                .collect::<Vec<_>>()
                        }, key = |(idx, _)| *idx {
                            text(class = "gds-notification-detail-line") {{entry_line.1}}
                        }
                        {(overflow_count > 0).then(|| {
                            let label_more = more_items_label(overflow_count);
                            let label_less = Gds::try_current()
                                .map_or_else(|| "Close".to_owned(), |gds| gds.i18n().text("close"));
                            view! {
                                Button(
                                    variant = ButtonVariant::Ghost,
                                    size = ButtonSize::Sm,
                                    on:click = move |ev: &mut EventCx<'_, zgui::view::events::Click>| {
                                        ev.stop_propagation();
                                        queue.toggle_expanded(id);
                                    }
                                ) {
                                    {move || if queue.is_expanded(id) { label_less.clone() } else { label_more.clone() }}
                                }
                            }
                            .into_view()
                        })}
                    }
                }
                .into_view()
            })}
        }
    })
}
