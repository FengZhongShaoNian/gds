use zgui::reactive::{Mounted, install, prelude::*};

use super::{Level, NotificationHost, NotificationQueue, Report, close_all, report};

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
