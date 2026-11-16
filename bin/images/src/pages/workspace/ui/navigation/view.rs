//! Workspace action bindings, keyboard cancellation, and zoom/fit navigation.

use gpui_kit::{Context, InteractiveElement, Window};

use super::*;
use crate::{
    features::{
        edit_canvas::ToolMode,
        navigation::{ZoomPreset, apply_zoom_preset},
    },
    shared::{
        geometry::{FitMode, Size2, ViewportTransform},
        ui::actions::{
            CancelInteraction, CopyImage, EditLocation, FirstImage, FlipHorizontal, FlipVertical,
            LastImage, MoveToTrash, NextImage, OpenClipboard, OpenFile, OpenSettings, OpenWithApp,
            PrevImage, QuitApp, RedoEdit, RevealInFiles, RotateCcw, RotateCw, SaveImage,
            SaveImageAs, SelectToolArrow, SelectToolBlur, SelectToolCrop, SelectToolEllipse,
            SelectToolLine, SelectToolMosaic, SelectToolPan, SelectToolPointer, SelectToolRect,
            SelectToolText, ToggleFilmstrip, ToggleInspector, UndoEdit, ZoomActual, ZoomFit,
            ZoomIn, ZoomOut,
        },
    },
};

macro_rules! bind_actions {
    ($root:expr, $cx:expr, { $($action:ty => |$this:ident, $win:ident, $acx:ident| $body:expr),* $(,)? }) => {{
        let mut div = $root;
        $(
            div = div.on_action($cx.listener(|$this, _: &$action, $win, $acx| $body));
        )*
        div
    }};
}

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn bind_workspace_actions<E: InteractiveElement>(
        &self,
        root: E,
        cx: &mut Context<Self>,
    ) -> E {
        bind_actions!(root, cx, {
            OpenFile => |this, win, cx| this.open_file_picker(win, cx),
            OpenClipboard => |this, win, cx| this.open_from_clipboard(win, cx),
            OpenSettings => |this, win, cx| this.open_settings(win, cx),
            PrevImage => |this, win, cx| this.navigate_sibling(-1, win, cx),
            NextImage => |this, win, cx| this.navigate_sibling(1, win, cx),
            FirstImage => |this, win, cx| this.navigate_edge(false, win, cx),
            LastImage => |this, win, cx| this.navigate_edge(true, win, cx),
            ZoomIn => |this, _win, cx| this.apply_zoom(ZoomPreset::ZoomIn, cx),
            ZoomOut => |this, _win, cx| this.apply_zoom(ZoomPreset::ZoomOut, cx),
            ZoomFit => |this, _win, cx| this.apply_zoom(ZoomPreset::Fit, cx),
            ZoomActual => |this, _win, cx| this.apply_zoom(ZoomPreset::Actual, cx),
            RotateCw => |this, _win, cx| this.apply_transform_op(TransformOp::RotateCw, cx),
            RotateCcw => |this, _win, cx| this.apply_transform_op(TransformOp::RotateCcw, cx),
            FlipHorizontal => |this, _win, cx| this.apply_transform_op(TransformOp::FlipH, cx),
            FlipVertical => |this, _win, cx| this.apply_transform_op(TransformOp::FlipV, cx),
            UndoEdit => |this, _win, cx| this.undo_edit(cx),
            RedoEdit => |this, _win, cx| this.redo_edit(cx),
            SaveImage => |this, win, cx| this.save_current_image(win, cx),
            SaveImageAs => |this, win, cx| this.save_image_as(win, cx),
            CopyImage => |this, win, cx| this.copy_to_clipboard(win, cx),
            ToggleFilmstrip => |this, _win, cx| this.toggle_filmstrip(cx),
            ToggleInspector => |this, _win, cx| this.toggle_inspector(cx),
            EditLocation => |this, win, cx| this.begin_path_edit(win, cx),
            RevealInFiles => |this, _win, cx| this.reveal_active_file(cx),
            OpenWithApp => |this, _win, cx| this.open_active_file_with(cx),
            MoveToTrash => |this, win, cx| this.move_active_to_trash(win, cx),
            SelectToolPointer => |this, win, cx| this.select_tool(ToolMode::Pointer, win, cx),
            SelectToolPan => |this, win, cx| this.select_tool(ToolMode::Pan, win, cx),
            SelectToolCrop => |this, win, cx| this.select_tool(ToolMode::Crop, win, cx),
            SelectToolRect => |this, win, cx| this.select_tool(ToolMode::Rect, win, cx),
            SelectToolEllipse => |this, win, cx| this.select_tool(ToolMode::Ellipse, win, cx),
            SelectToolArrow => |this, win, cx| this.select_tool(ToolMode::Arrow, win, cx),
            SelectToolLine => |this, win, cx| this.select_tool(ToolMode::Line, win, cx),
            SelectToolText => |this, win, cx| this.select_tool(ToolMode::Text, win, cx),
            SelectToolMosaic => |this, win, cx| this.select_tool(ToolMode::Mosaic, win, cx),
            SelectToolBlur => |this, win, cx| this.select_tool(ToolMode::Blur, win, cx),
            CancelInteraction => |this, win, cx| this.cancel_interaction(win, cx),
            QuitApp => |this, win, cx| this.on_quit_app_action(&QuitApp, win, cx),
        })
    }

    pub(in crate::pages::workspace::ui) fn on_close_settings_action(
        &mut self,
        _: &crate::shared::ui::actions::CloseSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_settings(window, cx);
    }

    pub(in crate::pages::workspace::ui) fn on_toggle_filmstrip_action(
        &mut self,
        _: &ToggleFilmstrip,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_filmstrip(cx);
    }

    pub(in crate::pages::workspace::ui) fn on_quit_app_action(
        &mut self,
        _: &QuitApp,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.quit();
    }

    pub(in crate::pages::workspace::ui) fn toggle_filmstrip(&mut self, cx: &mut Context<Self>) {
        self.chrome.filmstrip_expanded = !self.chrome.filmstrip_expanded;
        self.persist_chrome_preferences(cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn toggle_inspector(&mut self, cx: &mut Context<Self>) {
        self.chrome.inspector_visible = !self.chrome.inspector_visible;
        self.persist_chrome_preferences(cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn apply_zoom(
        &mut self,
        preset: ZoomPreset,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.session.document else {
            return;
        };
        let (w, h) = doc.output_dimensions();
        apply_zoom_preset(
            preset,
            &mut self.canvas.transform,
            self.chrome.last_viewport_size,
            Size2::new(w as f32, h as f32),
        );
        self.schedule_svg_rerender_if_needed(cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn apply_fit_mode(
        &mut self,
        mode: FitMode,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.session.document else {
            return;
        };
        let (w, h) = doc.output_dimensions();
        self.canvas.transform = ViewportTransform::compute_fit(
            mode,
            self.chrome.last_viewport_size,
            Size2::new(w as f32, h as f32),
        );
        self.schedule_svg_rerender_if_needed(cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn cancel_interaction(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.editing_path {
            self.cancel_path_edit(window, cx);
            return;
        }
        if self.chrome.app_menu_open {
            self.set_app_menu_open(false, cx);
            return;
        }
        if self.canvas.tool_popover.take().is_some() {
            self.canvas.color_picker_open = false;
            cx.notify();
            return;
        }
        if self.canvas.tool_fsm.cancel_active() {
            self.clear_text_overlay_input(window, cx);
            self.focus.canvas.focus(window, cx);
            cx.notify();
            return;
        }
        if self.canvas.tool_fsm.tool != ToolMode::Pointer {
            self.select_tool(ToolMode::Pointer, window, cx);
        }
    }
}
