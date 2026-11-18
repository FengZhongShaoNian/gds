use super::{Policy, WORKSPACE};
use crate::app::Gds;
use crate::service::task_manager::Scope;
use zgui::reactive::{Mounted, install};

#[test]
fn preference_off_on_restores_system_and_late_system_updates_cannot_enable() {
    let mut policy = Policy::default();
    for system in [false, true] {
        policy.system_reduced = system;
        policy.enabled = false;
        assert!(policy.reduced());
        policy.system_reduced = !system;
        assert!(policy.reduced());
        policy.enabled = true;
        assert_eq!(policy.reduced(), !system);
        policy.enabled = false;
        policy.system_reduced = system;
        policy.enabled = true;
        assert_eq!(policy.reduced(), system);
    }
}

#[test]
fn reactive_motion_zeroes_durations_in_css_when_reduced() {
    install().expect("install reactive runtime");
    let root = Mounted::new();
    root.with(|| {
        let gds = Gds::install(Scope::default());
        let motion = gds.appearance().motion();
        assert!(motion.enabled());
        assert_eq!(
            WORKSPACE.css_value(false),
            "160ms cubic-bezier(0, 0, 0.2, 1)"
        );
        assert_eq!(WORKSPACE.css_value(true), "0ms");

        let css = gds.appearance().gds_stylesheet_css();
        assert!(css.contains("--gds-motion-normal: 160ms;"));
        assert_eq!(
            gds.appearance().tokens_theme().motion.duration_normal,
            "160ms"
        );

        motion.set_enabled(false);
        assert!(!motion.enabled());
        let css = gds.appearance().gds_stylesheet_css();
        assert!(css.contains("--gds-motion-reduced: 1;"));
        assert!(css.contains("--gds-motion-fast: 0ms;"));
        assert!(css.contains("--gds-motion-normal: 0ms;"));
        assert_eq!(
            gds.appearance().tokens_theme().motion.duration_normal,
            "0ms"
        );
    });
    root.unmount();
}
