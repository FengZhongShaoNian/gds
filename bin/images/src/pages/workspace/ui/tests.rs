use std::time::Duration;

use gds::{app::AppExt as _, appearance::tokens};
use gpui_kit::{
    AppContext, Entity, TestAppContext, WindowHandle,
    component::{Root, WindowExt as _},
    px, size,
    test::TestWindowExt as _,
};

use super::*;
use crate::pages::workspace::{CaptureWorkflow, LaunchMode};

fn init_test_images(cx: &mut TestAppContext) -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("create temporary test directory");
    let settings_path = temp.path().join("settings.toml");
    cx.executor().allow_parking();
    cx.update(|cx| {
        cx.gds_mut().tasks().init_test(false);
        let tasks = cx.gds().tasks().scope();
        cx.gds_mut().init_ui(None);
        cx.gds_mut().appearance().motion().set_enabled(true);
        crate::shared::i18n::install(cx).expect("install i18n resources");
        let mut preferences = crate::entities::preferences::Preferences::default();
        preferences.appearance.source = gds::appearance::theme::Source::Builtin;
        cx.gds_mut()
            .appearance()
            .theme()
            .apply(&preferences.appearance)
            .expect("apply default theme");
        cx.gds_mut()
            .appearance()
            .typography()
            .apply(&preferences.font)
            .expect("apply default typography");
        cx.gds_mut()
            .appearance()
            .theme()
            .init_hot_reload(preferences.appearance.clone());
        cx.gds_mut()
            .preferences()
            .init_test(preferences, settings_path, &tasks);
        cx.gds_mut().ui().register_actions();
        crate::shared::ui::actions::bind_defaults(cx);
    });
    temp
}

fn open_test_workspace(
    cx: &mut TestAppContext,
    launch: LaunchOptions,
) -> (WindowHandle<Root>, Entity<WorkspacePage>) {
    let mut page = None;
    let window = cx.open_window(size(px(1280.0), px(800.0)), |window, cx| {
        let tasks = cx.gds().tasks().scope();
        gds::ui::notifications::bind_window(window, cx);
        let view = cx.new(|cx| WorkspacePage::new(tasks, launch, window, cx));
        page = Some(view.clone());
        let surface = gds::ui::notifications::surface::wrap(view, window, cx);
        Root::new(surface, window, cx)
    });
    (window, page.expect("workspace page created"))
}

#[gpui_kit::test]
async fn app_menu_stays_open_and_open_settings_outside_render_never_panics(
    cx: &mut TestAppContext,
) {
    let _temp = init_test_images(cx);
    let (window, page) = open_test_workspace(
        cx,
        LaunchOptions {
            mode: LaunchMode::EmptySmartState,
            workflow: CaptureWorkflow::default(),
            is_capture: false,
        },
    );
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("application-menu-slot", cx);
        window.render_frame(cx);
    })
    .expect("click application menu trigger");
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            page.read(cx).chrome.app_menu_open,
            "application menu must remain open across frames after click"
        );

        // Regression test for gpui-fast window.rs:5657 (`Window::current_view` panic when
        // `open_settings` runs outside `Render::render`) and entity_map double-lease in pages.
        page.update(cx, |page, cx| page.open_settings(window, cx));
        window.render_frame(cx);
        let settings_view = page
            .read(cx)
            .settings
            .view
            .clone()
            .expect("settings view initialized");
        assert!(page.read(cx).settings.open);
        assert!(!page.read(cx).chrome.app_menu_open);

        for target_page in [
            widgets::settings::Page::Viewer,
            widgets::settings::Page::Export,
            widgets::settings::Page::Appearance,
        ] {
            settings_view.update(cx, |view, cx| view.select_page(target_page, window, cx));
            window.render_frame(cx);
        }

        page.update(cx, |page, cx| page.toggle_filmstrip(cx));
        window.render_frame(cx);
    })
    .expect("open settings outside render, render all pages, and toggle sidebar");

    cx.executor().advance_clock(Duration::from_millis(180));
    cx.update_window(window.into(), |_, window, cx| {
        window.simulate_next_frame(cx);
        window.render_frame(cx);
        assert_eq!(
            page.read(cx).chrome.sidebar_presentation.width(),
            tokens::COLLAPSED_SIDEBAR_WIDTH
        );
        page.update(cx, |page, cx| page.close_settings(window, cx));
        window.render_frame(cx);
        assert!(!page.read(cx).settings.open);
        assert!(!window.has_active_dialog(cx));
    })
    .expect("settle settings sidebar toggle and close settings");

    let scope = cx.update(|cx| cx.gds().tasks().scope());
    cx.update_window(window.into(), |_, window, _| window.remove_window())
        .expect("remove test window");
    scope.shutdown().await;
}
