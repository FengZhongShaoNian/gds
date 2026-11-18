use zgui::reactive::{Mounted, install, prelude::*};

use super::{
    DETAIL_LIMIT, Level, NotificationHost, NotificationQueue, Operation, Report, close_all,
    close_current, focus, interaction, is_focused, report,
};

#[test]
fn notification_queue_records_and_clears_reports() {
    install().ok();
    let scope = Mounted::new();
    scope.with(|| {
        let _host = NotificationHost(zgui::view::Children::from_view(()));
        report(Report::new(Level::Error, "1 failed"));
        assert_eq!(NotificationQueue::active().items().get_untracked().len(), 1);
        close_all();
        assert!(
            NotificationQueue::active()
                .items()
                .get_untracked()
                .is_empty()
        );
    });
    scope.unmount();
}

#[test]
fn replacing_slot_updates_existing_notification_in_place() {
    struct SyncSlot;

    install().ok();
    let scope = Mounted::new();
    scope.with(|| {
        let _host = NotificationHost(zgui::view::Children::from_view(()));
        Report::new(Level::Info, "Copying 1 of 3")
            .replacing::<SyncSlot>()
            .show();
        Report::new(Level::Success, "Copied 3 of 3")
            .replacing::<SyncSlot>()
            .show();

        let items = NotificationQueue::active().items().get_untracked();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].report.summary, "Copied 3 of 3");
        assert_eq!(items[0].report.level, Level::Success);
    });
    scope.unmount();
}

#[test]
fn keyboard_navigation_focus_and_expand_details_track_selected_entry() {
    install().ok();
    let scope = Mounted::new();
    scope.with(|| {
        let _host = NotificationHost(zgui::view::Children::from_view(()));
        Report::new(Level::Info, "First").show();
        let details: Vec<String> = (1..=DETAIL_LIMIT + 2)
            .map(|i| format!("detail-{i}"))
            .collect();
        Report::new(Level::Error, "Second").details(details).show();

        let queue = NotificationQueue::active();
        let items = queue.items().get_untracked();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].report.summary, "Second");
        assert!(!items[0].autohide);
        assert!(items[1].autohide);

        focus();
        assert!(is_focused());
        assert_eq!(queue.selected_id().get_untracked(), Some(items[0].id));

        interaction::select_next();
        assert_eq!(queue.selected_id().get_untracked(), Some(items[1].id));

        assert!(!queue.is_expanded(items[0].id));
        queue.toggle_expanded(items[0].id);
        assert!(queue.is_expanded(items[0].id));

        close_current();
        assert_eq!(queue.items().get_untracked().len(), 1);
        assert_eq!(queue.items().get_untracked()[0].id, items[0].id);

        close_all();
        assert!(!is_focused());
    });
    scope.unmount();
}

#[test]
fn operation_builder_constructs_copyable_report() {
    let report = Operation::new(Level::Warning, "Batch completed")
        .item("alpha.png")
        .item("beta.png")
        .into_report();
    assert_eq!(report.level, Level::Warning);
    assert_eq!(report.details.len(), 2);
    assert_eq!(report.copy_text(), "Batch completed\nalpha.png\nbeta.png");
}
