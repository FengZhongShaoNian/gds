//! Transform commands, object editing, undo/redo, tool switching, crop, and text overlay.

use gpui_kit::{Context, Window};

use super::*;
use crate::{
    entities::document::RgbaColor,
    features::edit_canvas::{
        CancelOutcome, FinishOutcome, HoverTarget, MAX_UNDO_DEPTH, ReorderOp, StyleApplyOutcome,
        ToolMode,
    },
    shared::{
        geometry::{FitMode, Size2, ViewportTransform},
        i18n::text,
    },
};

const _: usize = MAX_UNDO_DEPTH;

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn flush_editing_text_before_external(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.is_editing_text() {
            self.commit_text_overlay(window, cx);
        }
    }

    pub(in crate::pages::workspace::ui) fn cancel_pointer_gesture_before_external(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.canvas.tool_fsm.is_pointer_gesture() {
            return;
        }
        let outcome = self
            .canvas
            .tool_fsm
            .cancel_pointer_gesture(&mut self.canvas.history);
        self.cancel_gesture_render();
        if outcome == CancelOutcome::NeedsRestoreComplete {
            self.restore_complete_presentation(cx);
        }
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn on_canvas_focus_lost(&mut self, cx: &mut Context<Self>) {
        self.canvas.history.break_coalesce();
        if self.canvas.panning {
            self.canvas.panning = false;
            self.canvas.last_pointer = None;
        }
        if self.canvas.tool_fsm.is_pointer_gesture() {
            self.cancel_pointer_gesture_before_external(cx);
        }
    }

    pub(in crate::pages::workspace::ui) fn apply_transform_op(
        &mut self,
        op: TransformOp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush_editing_text_before_external(window, cx);
        self.cancel_pointer_gesture_before_external(cx);
        let Some(doc) = &mut self.session.document else {
            return;
        };
        self.canvas
            .tool_fsm
            .apply_spatial_transform(op, doc, &mut self.canvas.history);
        let (w, h) = doc.output_dimensions();
        self.canvas.transform = ViewportTransform::compute_fit(
            FitMode::FitToWindow,
            self.chrome.last_viewport_size,
            Size2::new(w as f32, h as f32),
        );
        self.sync_window_title(window, cx);
        self.rebuild_textures(cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn undo_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.is_pointer_gesture() {
            self.cancel_pointer_gesture_before_external(cx);
            return;
        }
        let Some(doc) = &mut self.session.document else {
            return;
        };
        if !self.canvas.history.undo(doc) {
            return;
        }
        self.canvas.tool_fsm.selection.reconcile(&doc.annotations);
        let (w, h) = doc.output_dimensions();
        self.canvas.transform.recenter_if_fitted(
            self.chrome.last_viewport_size,
            Size2::new(w as f32, h as f32),
        );
        self.sync_window_title(window, cx);
        self.rebuild_textures(cx);
        show_report::<EditNotice>(Level::Info, text("toast-undo", cx), cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn redo_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.is_pointer_gesture() {
            self.cancel_pointer_gesture_before_external(cx);
            return;
        }
        let Some(doc) = &mut self.session.document else {
            return;
        };
        if !self.canvas.history.redo(doc) {
            return;
        }
        self.canvas.tool_fsm.selection.reconcile(&doc.annotations);
        let (w, h) = doc.output_dimensions();
        self.canvas.transform.recenter_if_fitted(
            self.chrome.last_viewport_size,
            Size2::new(w as f32, h as f32),
        );
        self.sync_window_title(window, cx);
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
        self.flush_editing_text_before_external(window, cx);
        let outcome = self
            .canvas
            .tool_fsm
            .set_tool(mode, &mut self.canvas.history);
        self.cancel_gesture_render();
        if outcome == CancelOutcome::NeedsRestoreComplete {
            self.restore_complete_presentation(cx);
        }
        self.canvas.hover_target = HoverTarget::None;
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

    pub(in crate::pages::workspace::ui) fn open_text_editor_with_value(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.text_overlay_input.update(cx, |input, cx| {
            input.set_value(value, window, cx);
            input.focus(window, cx);
        });
    }

    pub(in crate::pages::workspace::ui) fn on_text_draft_changed(
        &mut self,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.update_text_draft_content(value) {
            self.schedule_gesture_preview(cx);
            cx.notify();
        }
    }

    pub(in crate::pages::workspace::ui) fn apply_staged_crop(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        self.sync_window_title(window, cx);
        self.rebuild_textures(cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn clear_staged_crop(&mut self, cx: &mut Context<Self>) {
        self.canvas.tool_fsm.staged_crop = None;
        if self.canvas.tool_fsm.is_pointer_gesture() {
            self.cancel_pointer_gesture_before_external(cx);
        }
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn commit_text_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.canvas.tool_fsm.is_editing_text() {
            return;
        }
        let value = self.focus.text_overlay_input.read(cx).value().to_string();
        let Some(doc) = &mut self.session.document else {
            self.canvas.tool_fsm.cancel_active(&mut self.canvas.history);
            self.clear_text_overlay_input(window, cx);
            return;
        };
        let outcome = self
            .canvas
            .tool_fsm
            .commit_text_draft(&value, doc, &mut self.canvas.history);
        self.clear_text_overlay_input(window, cx);
        self.focus.canvas.focus(window, cx);
        self.cancel_gesture_render();
        match outcome {
            FinishOutcome::Committed => {
                self.sync_window_title(window, cx);
                self.rebuild_textures(cx);
            }
            _ => {
                self.restore_complete_presentation(cx);
            }
        }
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn cancel_text_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let outcome = self.canvas.tool_fsm.cancel_active(&mut self.canvas.history);
        self.cancel_gesture_render();
        self.clear_text_overlay_input(window, cx);
        self.focus.canvas.focus(window, cx);
        if outcome == CancelOutcome::NeedsRestoreComplete {
            self.restore_complete_presentation(cx);
        }
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn delete_canvas_object(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.is_pointer_gesture() {
            self.cancel_pointer_gesture_before_external(cx);
            return;
        }
        if self.canvas.tool_fsm.is_editing_text() {
            return;
        }
        let Some(doc) = &mut self.session.document else {
            return;
        };
        match self
            .canvas
            .tool_fsm
            .delete_selected_or_crop(doc, &mut self.canvas.history)
        {
            FinishOutcome::Committed => {
                self.sync_window_title(window, cx);
                self.rebuild_textures(cx);
                cx.notify();
            }
            FinishOutcome::SelectionChanged => {
                cx.notify();
            }
            _ => {}
        }
    }

    pub(in crate::pages::workspace::ui) fn duplicate_annotations(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.is_pointer_gesture() || self.canvas.tool_fsm.is_editing_text() {
            return;
        }
        let Some(doc) = &mut self.session.document else {
            return;
        };
        if self
            .canvas
            .tool_fsm
            .duplicate_selected(doc, &mut self.canvas.history)
            == FinishOutcome::Committed
        {
            self.sync_window_title(window, cx);
            self.rebuild_textures(cx);
            cx.notify();
        }
    }

    pub(in crate::pages::workspace::ui) fn select_all_annotations(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.is_pointer_gesture() || self.canvas.tool_fsm.is_editing_text() {
            return;
        }
        let Some(doc) = &self.session.document else {
            return;
        };
        if self
            .canvas
            .tool_fsm
            .select_all(doc, &mut self.canvas.history)
        {
            cx.notify();
        }
    }

    pub(in crate::pages::workspace::ui) fn handle_directional_key(
        &mut self,
        dx: f32,
        dy: f32,
        nav_delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.is_pointer_gesture() || self.canvas.tool_fsm.is_editing_text() {
            return;
        }
        if !self.canvas.tool_fsm.selection.is_empty() {
            let Some(doc) = &mut self.session.document else {
                return;
            };
            if self
                .canvas
                .tool_fsm
                .nudge_selection(dx, dy, doc, &mut self.canvas.history)
            {
                self.sync_window_title(window, cx);
                self.rebuild_textures(cx);
                cx.notify();
            }
            return;
        }
        self.navigate_sibling(nav_delta, window, cx);
    }

    pub(in crate::pages::workspace::ui) fn nudge_selection_fast(
        &mut self,
        dx: f32,
        dy: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.selection.is_empty()
            || self.canvas.tool_fsm.is_pointer_gesture()
            || self.canvas.tool_fsm.is_editing_text()
        {
            return;
        }
        let Some(doc) = &mut self.session.document else {
            return;
        };
        if self
            .canvas
            .tool_fsm
            .nudge_selection(dx, dy, doc, &mut self.canvas.history)
        {
            self.sync_window_title(window, cx);
            self.rebuild_textures(cx);
            cx.notify();
        }
    }

    pub(in crate::pages::workspace::ui) fn reorder_selection(
        &mut self,
        op: ReorderOp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.is_pointer_gesture() || self.canvas.tool_fsm.is_editing_text() {
            return;
        }
        let Some(doc) = &mut self.session.document else {
            return;
        };
        if self
            .canvas
            .tool_fsm
            .reorder_selection(op, doc, &mut self.canvas.history)
        {
            self.sync_window_title(window, cx);
            self.rebuild_textures(cx);
            cx.notify();
        }
    }

    pub(in crate::pages::workspace::ui) fn confirm_canvas_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.is_pointer_gesture() || self.canvas.tool_fsm.is_editing_text() {
            return;
        }
        if self.canvas.tool_fsm.staged_crop.is_some() {
            self.apply_staged_crop(window, cx);
            return;
        }
        if self.canvas.tool_fsm.selection.len() == 1
            && let Some(primary) = self.canvas.tool_fsm.selection.primary()
            && let Some(doc) = &self.session.document
            && let Some(initial) = self.canvas.tool_fsm.begin_edit_existing_text(
                primary,
                doc,
                &mut self.canvas.history,
            )
        {
            self.open_text_editor_with_value(initial, window, cx);
            cx.notify();
        }
    }

    pub(in crate::pages::workspace::ui) fn set_tool_color(
        &mut self,
        color: RgbaColor,
        cx: &mut Context<Self>,
    ) {
        let outcome = self.canvas.tool_fsm.apply_color(
            color,
            self.session.document.as_mut(),
            &mut self.canvas.history,
        );
        self.finish_style_apply(outcome, cx);
    }

    pub(in crate::pages::workspace::ui) fn set_tool_stroke_width(
        &mut self,
        width: f32,
        cx: &mut Context<Self>,
    ) {
        let outcome = self.canvas.tool_fsm.apply_stroke_width(
            width,
            self.session.document.as_mut(),
            &mut self.canvas.history,
        );
        self.finish_style_apply(outcome, cx);
    }

    pub(in crate::pages::workspace::ui) fn set_tool_font_size(
        &mut self,
        size: f32,
        cx: &mut Context<Self>,
    ) {
        let outcome = self.canvas.tool_fsm.apply_font_size(
            size,
            self.session.document.as_mut(),
            &mut self.canvas.history,
        );
        self.finish_style_apply(outcome, cx);
    }

    pub(in crate::pages::workspace::ui) fn set_tool_mosaic_block(
        &mut self,
        block_size: u32,
        cx: &mut Context<Self>,
    ) {
        let outcome = self.canvas.tool_fsm.apply_mosaic_block_size(
            block_size,
            self.session.document.as_mut(),
            &mut self.canvas.history,
        );
        self.finish_style_apply(outcome, cx);
    }

    pub(in crate::pages::workspace::ui) fn set_tool_blur_radius(
        &mut self,
        radius: u32,
        cx: &mut Context<Self>,
    ) {
        let outcome = self.canvas.tool_fsm.apply_blur_radius(
            radius,
            self.session.document.as_mut(),
            &mut self.canvas.history,
        );
        self.finish_style_apply(outcome, cx);
    }

    fn finish_style_apply(&mut self, outcome: StyleApplyOutcome, cx: &mut Context<Self>) {
        match outcome {
            StyleApplyOutcome::UpdatedTextDraft => {
                self.schedule_gesture_preview(cx);
            }
            StyleApplyOutcome::Committed => {
                self.rebuild_textures(cx);
            }
            StyleApplyOutcome::None => {}
        }
        cx.notify();
    }
}
