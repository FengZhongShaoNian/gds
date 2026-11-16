use std::{path::PathBuf, sync::Arc, time::Duration};

use gds::{app::AppExt as _, appearance::tokens};
use gpui_kit::{
    App, AppContext, CursorStyle, Entity, InputEvent as _, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, TestAppContext, Window, WindowHandle,
    component::{Root, WindowExt as _},
    point, px, size,
    test::{ClickOptions, TestAppContextExt as _, TestWindowExt as _},
};
use image::{Rgba, RgbaImage};

use super::*;
use crate::{
    entities::document::{AnnotationId, AnnotationItem, AnnotationKind, RgbaColor, SceneTag},
    features::{
        edit_canvas::{ActiveInteraction, HandleKind, PropertyValue, ToolMode, handles_for_item},
        export::{PixelSource, build_clipboard_plan, execute_clipboard_blocking},
    },
    pages::workspace::{CaptureWorkflow, LaunchMode},
    shared::geometry::{Point2, Rect2},
};

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

async fn open_png_file_workspace(
    cx: &mut TestAppContext,
    temp: &tempfile::TempDir,
    name: &str,
    w: u32,
    h: u32,
) -> (WindowHandle<Root>, Entity<WorkspacePage>, PathBuf) {
    let path = temp.path().join(name);
    let img = RgbaImage::from_pixel(w, h, Rgba([240, 240, 240, 255]));
    img.save(&path).expect("save sample png");
    let (window, page) = open_test_workspace(
        cx,
        LaunchOptions {
            mode: LaunchMode::OpenFile(path.clone()),
            workflow: CaptureWorkflow::default(),
            is_capture: false,
        },
    );
    let wait_page = page.clone();
    cx.wait_for(window.into(), Duration::from_secs(5), move |_, cx| {
        let p = wait_page.read(cx);
        p.session
            .document
            .as_ref()
            .is_some_and(|doc| doc.cache.complete_at(doc.edit_revision).is_some())
    })
    .await;
    (window, page, path)
}

async fn wait_for_complete_revision(
    cx: &mut TestAppContext,
    window: WindowHandle<Root>,
    page: &Entity<WorkspacePage>,
    revision: u64,
) {
    let wait_page = page.clone();
    cx.wait_for(window.into(), Duration::from_secs(5), move |_, cx| {
        wait_page
            .read(cx)
            .session
            .document
            .as_ref()
            .is_some_and(|doc| {
                doc.cache.complete_at(revision).is_some()
                    && doc
                        .cache
                        .presented
                        .as_ref()
                        .is_some_and(|scene| scene.tag == SceneTag::Complete { revision })
            })
    })
    .await;
}

async fn wait_for_transient_scene(
    cx: &mut TestAppContext,
    window: WindowHandle<Root>,
    page: &Entity<WorkspacePage>,
) {
    let wait_page = page.clone();
    cx.wait_for(window.into(), Duration::from_secs(5), move |_, cx| {
        wait_page
            .read(cx)
            .session
            .document
            .as_ref()
            .and_then(|doc| doc.cache.presented.as_ref())
            .is_some_and(|scene| matches!(scene.tag, SceneTag::Transient { .. }))
    })
    .await;
}

fn img_to_window(
    window: &Window,
    page: &Entity<WorkspacePage>,
    cx: &App,
    pt: Point2,
) -> Point<Pixels> {
    let bounds = window.find("images-viewport").bounds();
    let vp = page.read(cx).canvas.transform.image_to_viewport(pt);
    point(bounds.origin.x + px(vp.x), bounds.origin.y + px(vp.y))
}

fn img_to_viewport_offset(page: &Entity<WorkspacePage>, cx: &App, pt: Point2) -> Point<Pixels> {
    let vp = page.read(cx).canvas.transform.image_to_viewport(pt);
    point(px(vp.x), px(vp.y))
}

fn simulate_mouse_drag_start(
    window: &mut Window,
    from: Point<Pixels>,
    to: Point<Pixels>,
    cx: &mut App,
) {
    window.dispatch_event(
        MouseMoveEvent {
            position: from,
            pressed_button: None,
            modifiers: Modifiers::default(),
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(
        MouseDownEvent {
            position: from,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(
        MouseMoveEvent {
            position: to,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
}

fn simulate_mouse_release(window: &mut Window, at: Point<Pixels>, cx: &mut App) {
    window.dispatch_event(
        MouseUpEvent {
            position: at,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
}

fn decode_clipboard_png_rgba(doc: &DocumentState) -> RgbaImage {
    let plan = build_clipboard_plan(doc, None, &Default::default());
    let png = execute_clipboard_blocking(plan).expect("encode clipboard png");
    image::load_from_memory(&png)
        .expect("decode clipboard png")
        .to_rgba8()
}

fn select_all_settings_pages_and_toggle(
    settings_view: &Entity<widgets::settings::View>,
    page: &Entity<WorkspacePage>,
    window: &mut Window,
    cx: &mut App,
) {
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
        select_all_settings_pages_and_toggle(&settings_view, &page, window, cx);
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

#[gpui_kit::test]
async fn r1_delete_repeat_is_safe_and_ctrl_delete_prompts_on_dirty_doc(cx: &mut TestAppContext) {
    let temp = init_test_images(cx);
    let (window, page, path) = open_png_file_workspace(cx, &temp, "r1.png", 160, 120).await;

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.press("4", cx);
        let from = img_to_window(window, &page, cx, Point2::new(20.0, 20.0));
        let to = img_to_window(window, &page, cx, Point2::new(80.0, 60.0));
        window.drag(from, to, cx);
        assert_eq!(page.read(cx).canvas.tool_fsm.tool, ToolMode::Rect);
        assert_eq!(page.read(cx).canvas.tool_fsm.selection.len(), 1);
    })
    .expect("draw rect annotation");

    wait_for_complete_revision(cx, window, &page, 1).await;

    cx.update_window(window.into(), |_, window, cx| {
        window.press("delete", cx);
        window.press("delete", cx);
        window.press("delete", cx);
        let p = page.read(cx);
        let doc = p.session.document.as_ref().expect("doc remains loaded");
        assert!(doc.annotations.is_empty());
        assert!(p.canvas.tool_fsm.selection.is_empty());
        assert!(!window.has_active_dialog(cx));
        assert!(path.exists(), "repeated bare Delete must never trash file");

        // Make document dirty and trigger `ctrl-delete`: `confirm_discard_changes` must guard it.
        window.press("r", cx);
        assert!(page.read(cx).has_unsaved_changes());
        window.press("ctrl-delete", cx);
        assert!(
            window.has_active_dialog(cx),
            "Ctrl+Delete on dirty document must require discard confirmation"
        );
        assert!(path.exists());
    })
    .expect("verify delete safety and ctrl-delete discard dialog");

    let scope = cx.update(|cx| cx.gds().tasks().scope());
    cx.update_window(window.into(), |_, window, _| window.remove_window())
        .expect("remove test window");
    scope.shutdown().await;
}

#[gpui_kit::test]
async fn r2_r3_r4_mid_drag_stacking_blur_handoff_and_export_isolation(cx: &mut TestAppContext) {
    let temp = init_test_images(cx);
    let (window, page, _) = open_png_file_workspace(cx, &temp, "r234.png", 160, 120).await;

    let initial_complete_ptr: usize = cx
        .update_window(window.into(), |_, window, cx| {
            page.update(cx, |this, cx| {
                let doc = this.session.document.as_mut().expect("doc loaded");
                doc.annotations.push(AnnotationItem {
                    id: AnnotationId(1),
                    kind: AnnotationKind::Rect {
                        rect: Rect2::new(10.0, 20.0, 24.0, 24.0),
                        stroke: RgbaColor::RED,
                        stroke_width: 4.0,
                        fill: Some(RgbaColor::RED),
                    },
                });
                doc.annotations.push(AnnotationItem {
                    id: AnnotationId(2),
                    kind: AnnotationKind::Text {
                        position: [45.0, 25.0],
                        content: "Top".to_owned(),
                        color: RgbaColor::BLUE,
                        font_size: 18.0,
                    },
                });
                doc.annotations.push(AnnotationItem {
                    id: AnnotationId(3),
                    kind: AnnotationKind::GaussianBlur {
                        region: Rect2::new(70.0, 15.0, 50.0, 50.0),
                        radius: 4,
                    },
                });
                doc.next_annotation_id = 4;
                doc.bump_revision();
                this.rebuild_textures(cx);
            });
            window.render_frame(cx);
            0usize
        })
        .expect("seed stacked annotations");
    let _ = initial_complete_ptr;

    wait_for_complete_revision(cx, window, &page, 1).await;

    let (complete_before_drag, drag_start, drag_mid) = cx
        .update_window(window.into(), |_, window, cx| {
            let p = page.read(cx);
            let doc = p.session.document.as_ref().expect("doc");
            let complete_before = Arc::clone(
                doc.cache
                    .complete_at(doc.edit_revision)
                    .expect("complete ready"),
            );
            let from = img_to_window(window, &page, cx, Point2::new(22.0, 20.0));
            let mid = img_to_window(window, &page, cx, Point2::new(92.0, 35.0));
            simulate_mouse_drag_start(window, from, mid, cx);
            assert!(matches!(
                page.read(cx).canvas.tool_fsm.interaction,
                ActiveInteraction::Moving { .. }
            ));
            (complete_before, from, mid)
        })
        .expect("begin dragging bottom rect under text & blur");

    wait_for_transient_scene(cx, window, &page).await;

    cx.update_window(window.into(), |_, window, cx| {
        let p = page.read(cx);
        let doc = p.session.document.as_ref().expect("doc");
        // R2: Document stacking order remains [Rect(1), Text(2), Blur(3)] during drag.
        assert_eq!(doc.annotations[0].id, AnnotationId(1));
        assert_eq!(doc.annotations[1].id, AnnotationId(2));
        assert_eq!(doc.annotations[2].id, AnnotationId(3));
        let presented = doc.cache.presented_annotations(&doc.annotations);
        assert_eq!(presented[0].id, AnnotationId(1));
        assert_eq!(presented[1].id, AnnotationId(2));
        assert!(presented[0].visual_bounds().x > 50.0);

        // R4: `install_transient` never touches `complete`; export during drag exports pre-drag commit.
        let complete_now = doc
            .cache
            .complete_at(doc.edit_revision)
            .expect("complete still cached");
        assert!(Arc::ptr_eq(&complete_before_drag, complete_now));
        let exported = decode_clipboard_png_rgba(doc);
        assert_eq!(*exported.get_pixel(22, 32), Rgba([239, 68, 68, 255]));

        // R3 cancel: pressing Escape restores `Complete(1)` without altering annotations or history.
        window.press("escape", cx);
        assert!(matches!(
            page.read(cx).canvas.tool_fsm.interaction,
            ActiveInteraction::Idle
        ));
    })
    .expect("verify mid-drag isolation and escape cancel");

    wait_for_complete_revision(cx, window, &page, 1).await;

    // R3 commit handoff: drag & release retains last Transient frame immediately until Complete(2) lands.
    cx.update_window(window.into(), |_, window, cx| {
        simulate_mouse_drag_start(window, drag_start, drag_mid, cx);
    })
    .expect("start second drag for commit handoff");
    wait_for_transient_scene(cx, window, &page).await;
    cx.update_window(window.into(), |_, window, cx| {
        simulate_mouse_release(window, drag_mid, cx);
        let p = page.read(cx);
        let doc = p.session.document.as_ref().expect("doc");
        assert_eq!(doc.edit_revision, 2);
        assert!(
            doc.cache.presented_output().is_some(),
            "presented frame must never vanish between MouseUp and Complete(R+1)"
        );
        // Immediate export right after commit before Complete(2) finishes still builds updated pixels.
        assert!(matches!(
            PixelSource::from_document(doc),
            PixelSource::NeedsComposite { .. }
        ));
        let exported = decode_clipboard_png_rgba(doc);
        assert_eq!(*exported.get_pixel(22, 32), Rgba([240, 240, 240, 255]));
    })
    .expect("verify atomic commit handoff and immediate export");

    wait_for_complete_revision(cx, window, &page, 2).await;

    let scope = cx.update(|cx| cx.gds().tasks().scope());
    cx.update_window(window.into(), |_, window, _| window.remove_window())
        .expect("remove test window");
    scope.shutdown().await;
}

#[gpui_kit::test]
async fn r5_r6_mixed_inspector_rigid_clamp_text_draft_and_cursor_mapping(cx: &mut TestAppContext) {
    let temp = init_test_images(cx);
    let (window, page, _) = open_png_file_workspace(cx, &temp, "r56.png", 200, 120).await;

    cx.update_window(window.into(), |_, window, cx| {
        page.update(cx, |this, cx| {
            let doc = this.session.document.as_mut().expect("doc");
            doc.annotations.push(AnnotationItem {
                id: AnnotationId(1),
                kind: AnnotationKind::Rect {
                    rect: Rect2::new(10.0, 15.0, 24.0, 20.0),
                    stroke: RgbaColor::RED,
                    stroke_width: 3.0,
                    fill: None,
                },
            });
            doc.annotations.push(AnnotationItem {
                id: AnnotationId(2),
                kind: AnnotationKind::Rect {
                    rect: Rect2::new(50.0, 15.0, 24.0, 20.0),
                    stroke: RgbaColor::BLUE,
                    stroke_width: 3.0,
                    fill: None,
                },
            });
            doc.annotations.push(AnnotationItem {
                id: AnnotationId(3),
                kind: AnnotationKind::Text {
                    position: [40.0, 60.0],
                    content: "iiii WWWW\n中文".to_owned(),
                    color: RgbaColor::BLACK,
                    font_size: 20.0,
                },
            });
            doc.next_annotation_id = 4;
            doc.bump_revision();
            this.canvas.tool_fsm.selection.set_single(AnnotationId(1));
            this.canvas.tool_fsm.selection.toggle(AnnotationId(2));
            this.rebuild_textures(cx);
        });
        window.render_frame(cx);

        let p = page.read(cx);
        let doc = p.session.document.as_ref().expect("doc");
        let summary = p
            .canvas
            .tool_fsm
            .selection_style_summary(&doc.annotations)
            .expect("selection summary");
        assert_eq!(summary.color, PropertyValue::Mixed);
        assert_eq!(p.canvas.tool_fsm.settings.color, RgbaColor::RED);
        assert!(window.try_find("inspector-selection").is_some());

        // Rigid group edge clamping via fast nudge.
        window.press("shift-left", cx);
        window.press("shift-left", cx);
        let doc = page.read(cx).session.document.as_ref().expect("doc");
        assert_eq!(doc.annotations[0].visual_bounds().x, 0.0);
        assert_eq!(doc.annotations[1].visual_bounds().x, 40.0);

        // Double-click existing Text -> edit + change color -> Esc discards both cleanly.
        let text_center = doc.annotations[2].visual_bounds().center();
        let offset = img_to_viewport_offset(&page, cx, text_center);
        window.click_with_options(
            "images-viewport",
            ClickOptions::new().with_offset(offset).with_count(2),
            cx,
        );
        window.render_frame(cx);
        assert!(page.read(cx).canvas.tool_fsm.is_editing_text());
        assert!(window.try_find("text-overlay-box").is_some());

        page.update(cx, |this, cx| {
            this.focus
                .text_overlay_input
                .update(cx, |input, cx| input.set_value("Changed", window, cx));
            this.set_tool_color(RgbaColor::GREEN, cx);
        });
        window.press("escape", cx);
        assert!(!page.read(cx).canvas.tool_fsm.is_editing_text());
        assert_eq!(
            page.read(cx)
                .session
                .document
                .as_ref()
                .expect("doc")
                .annotations[2]
                .color(),
            Some(RgbaColor::BLACK)
        );

        // Re-open Text edit, change text & color, and commit via `flush_editing_text_before_external`
        // (simulating immediate Ctrl+C after typing) -> single undo restores both together.
        window.click_with_options(
            "images-viewport",
            ClickOptions::new().with_offset(offset).with_count(2),
            cx,
        );
        window.render_frame(cx);
        page.update(cx, |this, cx| {
            this.focus.text_overlay_input.update(cx, |input, cx| {
                input.set_value("Line A\nLine B", window, cx)
            });
            this.set_tool_color(RgbaColor::GREEN, cx);
            this.flush_editing_text_before_external(window, cx);
        });
        assert_eq!(
            page.read(cx)
                .session
                .document
                .as_ref()
                .expect("doc")
                .annotations[2]
                .color(),
            Some(RgbaColor::GREEN)
        );
        window.press("ctrl-z", cx);
        assert_eq!(
            page.read(cx)
                .session
                .document
                .as_ref()
                .expect("doc")
                .annotations[2]
                .color(),
            Some(RgbaColor::BLACK)
        );

        // Verify 4-corner Text handles & gpui-fast diagonal CursorStyle mapping.
        page.update(cx, |this, cx| {
            let doc = this.session.document.as_ref().expect("doc");
            let text_item = &doc.annotations[2];
            assert_eq!(handles_for_item(text_item).len(), 4);
            this.canvas.tool_fsm.selection.set_single(AnnotationId(3));
            this.canvas.hover_target = crate::features::edit_canvas::HoverTarget::Handle {
                id: AnnotationId(3),
                handle: HandleKind::Nw,
            };
            assert_eq!(
                this.viewport_cursor_style(),
                CursorStyle::ResizeUpRightDownLeft
            );
            this.canvas.hover_target = crate::features::edit_canvas::HoverTarget::Handle {
                id: AnnotationId(3),
                handle: HandleKind::Ne,
            };
            assert_eq!(
                this.viewport_cursor_style(),
                CursorStyle::ResizeUpLeftDownRight
            );
            cx.notify();
        });
    })
    .expect("verify mixed inspector, rigid clamping, text draft, and cursor style");

    let final_rev = cx.read(|cx| {
        page.read(cx)
            .session
            .document
            .as_ref()
            .expect("doc")
            .edit_revision
    });
    assert_eq!(final_rev, 4);
    wait_for_complete_revision(cx, window, &page, final_rev).await;

    let scope = cx.update(|cx| cx.gds().tasks().scope());
    cx.update_window(window.into(), |_, window, _| window.remove_window())
        .expect("remove test window");
    scope.shutdown().await;
}
