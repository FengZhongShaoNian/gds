use std::time::{Duration, Instant};

use gds::{app::AppExt as _, appearance::tokens, service::image::source::Origin};
use gpui_kit::{
    AppContext, Entity, TestAppContext, WindowHandle,
    component::{Root, WindowExt as _},
    px, size,
    test::TestWindowExt as _,
};
use image::{Rgba, RgbaImage};

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

fn write_sample_png(path: &std::path::Path, rgba: [u8; 4]) {
    RgbaImage::from_pixel(24, 24, Rgba(rgba))
        .save(path)
        .expect("write sample png");
}

#[gpui_kit::test]
async fn workspace_layout_chevrons_and_inspector_motion(cx: &mut TestAppContext) {
    let temp = init_test_images(cx);
    let gallery = temp.path().join("gallery");
    std::fs::create_dir_all(&gallery).expect("create gallery directory");
    let first = gallery.join("01-first.png");
    let second = gallery.join("02-second.png");
    let third = gallery.join("03-third.png");
    write_sample_png(&first, [220, 40, 40, 255]);
    write_sample_png(&second, [40, 200, 80, 255]);
    write_sample_png(&third, [60, 120, 230, 255]);

    let (window, page) = open_test_workspace(
        cx,
        LaunchOptions {
            mode: LaunchMode::OpenFile(first.clone()),
            workflow: CaptureWorkflow::default(),
            is_capture: false,
        },
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        let ready = cx.read(|cx| {
            let view = page.read(cx);
            view.session.document.is_some()
                && view.session.catalog.entries.len() == 3
                && !view.runtime.is_loading
        });
        if ready || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let initial_viewport_w = cx
        .update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);

            let view = page.read(cx);
            assert!(view.session.document.is_some());
            assert_eq!(view.session.catalog.entries.len(), 3);
            assert!(matches!(
                view.session.document.as_ref().map(|d| &d.decoded.origin),
                Some(Origin::LocalFile { path, .. }) if path == &first
            ));

            let vp_bounds = window.find("images-viewport").bounds();
            assert!(
                f32::from(vp_bounds.size.height) >= 700.0,
                "viewport must fill full workspace height, got {}",
                f32::from(vp_bounds.size.height)
            );
            assert!(window.try_find("images-filmstrip").is_some());
            assert!(
                window.try_find("filmstrip-prev-slot").is_some(),
                "up chevron must remain visible even for 3 images"
            );
            assert!(
                window.try_find("filmstrip-next-slot").is_some(),
                "down chevron must remain visible even for 3 images"
            );
            assert!(window.try_find("location-display").is_some());
            assert!(window.try_find("location-actions").is_some());
            assert!(window.try_find("inspector-edit").is_some());
            assert!(window.try_find("inspector-metadata").is_some());
            assert!(window.try_find("images-inspector").is_some());

            f32::from(vp_bounds.size.width)
        })
        .expect("verify initial workspace geometry");

    cx.update_window(window.into(), |_, window, cx| {
        page.update(cx, |page, cx| page.toggle_filmstrip(cx));
        window.render_frame(cx);
    })
    .expect("collapse filmstrip");

    cx.executor().advance_clock(Duration::from_millis(60));
    cx.update_window(window.into(), |_, window, cx| {
        assert!(window.simulate_next_frame(cx) >= 1);
    })
    .expect("advance filmstrip midpoint");
    cx.run_until_parked();

    cx.update_window(window.into(), |_, _, cx| {
        let width = page.read(cx).chrome.sidebar_presentation.width();
        assert!(
            width > tokens::COLLAPSED_SIDEBAR_WIDTH && width < tokens::SIDEBAR_WIDTH,
            "expected midpoint width, got {width}"
        );
    })
    .expect("verify midpoint filmstrip animation");

    cx.executor().advance_clock(Duration::from_millis(140));
    cx.update_window(window.into(), |_, window, cx| {
        window.simulate_next_frame(cx);
    })
    .expect("settle filmstrip collapse");
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        assert_eq!(
            page.read(cx).chrome.sidebar_presentation.width(),
            tokens::COLLAPSED_SIDEBAR_WIDTH
        );
        assert!(window.try_find("filmstrip-prev-slot").is_some());
        assert!(window.try_find("filmstrip-next-slot").is_some());

        page.update(cx, |page, cx| page.toggle_inspector(cx));
        window.render_frame(cx);
    })
    .expect("collapse inspector");

    cx.executor().advance_clock(Duration::from_millis(60));
    cx.update_window(window.into(), |_, window, cx| {
        assert!(window.simulate_next_frame(cx) >= 1);
        window.render_frame(cx);
        let mid_inspector = window.find("images-inspector").bounds();
        assert!(
            f32::from(mid_inspector.size.width) > 10.0
                && f32::from(mid_inspector.size.width)
                    < f32::from(px(DEFAULT_INSPECTOR_WIDTH as f32)),
            "expected inspector animation midpoint, got {}",
            f32::from(mid_inspector.size.width)
        );
    })
    .expect("verify midpoint inspector animation");

    cx.executor().advance_clock(Duration::from_millis(160));
    cx.update_window(window.into(), |_, window, cx| {
        window.simulate_next_frame(cx);
        window.render_frame(cx);
        assert!(window.try_find("images-inspector").is_none());
        let expanded_vp_w = f32::from(window.find("images-viewport").bounds().size.width);
        assert!(
            expanded_vp_w > initial_viewport_w + 150.0,
            "viewport should expand cleanly when sidebars collapse ({initial_viewport_w} -> {expanded_vp_w})"
        );
    })
    .expect("verify settled inspector collapse");

    let scope = cx.update(|cx| cx.gds().tasks().scope());
    cx.update_window(window.into(), |_, window, _| window.remove_window())
        .expect("remove test window");
    scope.shutdown().await;
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
