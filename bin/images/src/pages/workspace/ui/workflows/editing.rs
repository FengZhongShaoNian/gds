//! Transform commands, undo/redo, tool mode selection, crop apply/clear, and text overlay.

use std::sync::Arc;

use gpui_kit::{Context, Window};

use super::*;
use crate::{
    entities::document::RgbaColor,
    features::edit_canvas::{EditCommand, MAX_UNDO_DEPTH, ToolMode},
    shared::{
        geometry::{FitMode, Size2, ViewportTransform},
        i18n::text,
    },
};

const _: usize = MAX_UNDO_DEPTH;

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn apply_transform_op(
        &mut self,
        op: TransformOp,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &mut self.session.document else {
            return;
        };
        let prev_annotations = Arc::from(doc.annotations.as_slice());
        let command = match op {
            TransformOp::RotateCw => EditCommand::RotateCw { prev_annotations },
            TransformOp::RotateCcw => EditCommand::RotateCcw { prev_annotations },
            TransformOp::FlipH => EditCommand::FlipHorizontal { prev_annotations },
            TransformOp::FlipV => EditCommand::FlipVertical { prev_annotations },
        };
        self.canvas.history.apply(command, doc);
        let (w, h) = doc.output_dimensions();
        self.canvas.transform = ViewportTransform::compute_fit(
            FitMode::FitToWindow,
            self.chrome.last_viewport_size,
            Size2::new(w as f32, h as f32),
        );
        self.rebuild_textures(cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn undo_edit(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.session.document else {
            return;
        };
        if !self.canvas.history.undo(doc) {
            return;
        }
        let (w, h) = doc.output_dimensions();
        self.canvas.transform.recenter_if_fitted(
            self.chrome.last_viewport_size,
            Size2::new(w as f32, h as f32),
        );
        self.rebuild_textures(cx);
        show_report::<EditNotice>(Level::Info, text("toast-undo", cx), cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn redo_edit(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.session.document else {
            return;
        };
        if !self.canvas.history.redo(doc) {
            return;
        }
        let (w, h) = doc.output_dimensions();
        self.canvas.transform.recenter_if_fitted(
            self.chrome.last_viewport_size,
            Size2::new(w as f32, h as f32),
        );
        self.rebuild_textures(cx);
        show_report::<EditNotice>(Level::Info, text("toast-redo", cx), cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn select_tool(
        &mut self,
        mode: ToolMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let leaving_text = self.canvas.tool_fsm.tool == ToolMode::Text && mode != ToolMode::Text;
        if leaving_text {
            self.canvas.tool_fsm.pending_text_pos = None;
            self.clear_text_overlay_input(window, cx);
        }
        self.canvas.tool_fsm.set_tool(mode);
        self.canvas.tool_popover = None;
        self.canvas.color_picker_open = false;
        self.focus.canvas.focus(window, cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn clear_text_overlay_input(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus
            .text_overlay_input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    pub(in crate::pages::workspace::ui) fn apply_staged_crop(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.session.document else {
            return;
        };
        if !self
            .canvas
            .tool_fsm
            .apply_staged_crop(doc, &mut self.canvas.history)
        {
            return;
        }
        let (w, h) = doc.output_dimensions();
        self.canvas.transform = ViewportTransform::compute_fit(
            FitMode::FitToWindow,
            self.chrome.last_viewport_size,
            Size2::new(w as f32, h as f32),
        );
        self.rebuild_textures(cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn clear_staged_crop(&mut self, cx: &mut Context<Self>) {
        self.canvas.tool_fsm.staged_crop = None;
        self.canvas.tool_fsm.drag = None;
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn commit_text_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.focus.text_overlay_input.read(cx).value().to_string();
        let Some(doc) = &mut self.session.document else {
            return;
        };
        let committed = self
            .canvas
            .tool_fsm
            .commit_text(&value, doc, &mut self.canvas.history);
        self.clear_text_overlay_input(window, cx);
        self.focus.canvas.focus(window, cx);
        if committed {
            self.rebuild_textures(cx);
        }
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn cancel_text_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.canvas.tool_fsm.pending_text_pos = None;
        self.clear_text_overlay_input(window, cx);
        self.focus.canvas.focus(window, cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn set_tool_color(
        &mut self,
        color: RgbaColor,
        cx: &mut Context<Self>,
    ) {
        self.canvas.tool_fsm.settings.color = color;
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn set_tool_stroke_width(
        &mut self,
        width: f32,
        cx: &mut Context<Self>,
    ) {
        self.canvas.tool_fsm.settings.stroke_width = width;
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn set_tool_font_size(
        &mut self,
        size: f32,
        cx: &mut Context<Self>,
    ) {
        self.canvas.tool_fsm.settings.font_size = size;
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn set_tool_mosaic_block(
        &mut self,
        block_size: u32,
        cx: &mut Context<Self>,
    ) {
        self.canvas.tool_fsm.settings.mosaic_block_size = block_size;
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn set_tool_blur_radius(
        &mut self,
        radius: u32,
        cx: &mut Context<Self>,
    ) {
        self.canvas.tool_fsm.settings.blur_radius = radius;
        cx.notify();
    }
}
