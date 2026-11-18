use zgui::prelude::*;
use zgui::reactive::{Mounted, install};
use zgui::view;

use crate::app::Gds;
use crate::i18n::Language;
use crate::service::task_manager::Scope;

use super::{APPLICATIONS, AboutProps, Application, HEIGHT, WIDTH, is_http_url, localized};

#[test]
fn registered_files_application_metadata_is_present() {
    let app = Application::cargo("gds-files");
    assert_eq!(app.name, "GDS Files");
    assert!(app.artwork_svg().is_some_and(|svg| svg.contains("<svg")));
    assert_eq!(WIDTH, 480.0);
    assert_eq!(HEIGHT, 360.0);
    assert!(!APPLICATIONS.is_empty());
}

#[test]
fn about_url_guard_and_localized_strings_work_with_and_without_gds() {
    assert!(is_http_url("https://example.com"));
    assert!(is_http_url(" http://localhost:8080 "));
    assert!(!is_http_url("file:///etc/passwd"));
    assert!(!is_http_url("javascript:alert(1)"));
    assert!(!is_http_url(""));

    install().ok();
    let scope = Mounted::new();
    scope.with(|| {
        assert_eq!(localized("gds-about-back", "Back"), "Back");
        let gds = Gds::install(Scope::default());
        let credits = Signal::derive_local(|| localized("gds-about-credits", "Fallback"));
        let license = Signal::derive_local(|| localized("gds-about-license", "Fallback"));

        gds.i18n()
            .install(Language::English)
            .expect("English locale");
        assert_eq!(credits.get_untracked(), "Credits");
        assert_eq!(license.get_untracked(), "License");

        gds.i18n()
            .install(Language::Chinese)
            .expect("Chinese locale");
        assert_eq!(credits.get_untracked(), "致谢");
        assert_eq!(license.get_untracked(), "许可证");

        let _about_view = view! { About(application = Application::cargo("gds-files")) };
    });
    scope.unmount();
}
