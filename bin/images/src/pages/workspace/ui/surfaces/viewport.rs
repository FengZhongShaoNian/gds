//! Center viewport canvas: checkerboard, presented GPU output, chrome-only overlays,
//! dynamic cursor feedback, and multiline IME text overlay.

use std::{cell::Cell, rc::Rc, sync::Arc};

use gds::{
    app::AppExt as _,
    service::image::source::Origin,
    ui::{button::Button, file_inspection::State as InspectionState},
};
use gds_uim::el;
use gpui_kit::{
    BorderStyle, Bounds, Context, CursorStyle, ExternalPaths, Hsla, IntoElement, Modifiers,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, PathBuilder, Point,
    ScrollDelta, ScrollEvent, TestSupportExt as _, Window,
    assets::IconName,
    canvas,
    component::{
        ActiveTheme as _, Icon, Sizable as _, StyledExt as _, button::ButtonVariants as _,
        input::Textarea, spinner::Spinner,
    },
    div, fill, img, outline, point,
    prelude::*,
    px, size,
};

use super::*;
use crate::{
    entities::{
        document::{AnnotationItem, RenderOutput},
        preferences::PreferencesExt as _,
    },
    features::edit_canvas::{
        ActiveInteraction, FinishOutcome, HandleKind, HoverTarget, PointerDownInput,
        PointerDownOutcome, PointerModifiers, PointerMoveOutcome, TextEditTarget, ToolMode,
        handles_for_item, hover_test_canvas,
    },
    pages::workspace::ui::navigation::catalog::entry_inspection_source,
    shared::{
        clipboard::ClipboardOfferKind,
        geometry::{Point2, Rect2, Size2, ViewportTransform},
        i18n::text,
        ui::actions::CANVAS_CONTEXT,
    },
};

const CHECKER_TILE: f32 = 12.0;
const HANDLE_DRAW_SIZE: f32 = 8.0;

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn render_viewport(
        &mut self,
        vp_size: Size2,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let vp_origin_cell = Rc::new(Cell::new(Point::default()));
        let origin_tracker = {
            let cell = vp_origin_cell.clone();
            canvas(
                move |bounds, _, _| {
                    cell.set(bounds.origin);
                },
                move |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
        };

        if self.runtime.is_loading && self.session.document.is_none() {
            return el! {
                .div {
                    # "images-viewport";
                    test_support;
                    size_full;
                    relative;
                    overflow_hidden;
                    flex;
                    items_center;
                    justify_center;
                } [
                    origin_tracker,
                    $(self.render_loading_preview(cx)),
                ]
            }
            .into_any_element();
        }

        let Some(doc) = &self.session.document else {
            return el! {
                .div {
                    # "images-viewport";
                    test_support;
                    size_full;
                    relative;
                    overflow_hidden;
                    on_drop: $(cx.listener(Self::on_external_drop));
                    drag_over: $(|style, _: &ExternalPaths, _, cx| style.bg(cx.theme().drop_target));
                } [
                    origin_tracker,
                    $(self.render_empty_state(cx)),
                ]
            }
            .into_any_element();
        };

        let (out_w, out_h) = doc.output_dimensions();
        let accessible_name = doc.decoded.origin.display_name();
        let tf = self.canvas.transform;
        let img_vp = tf.image_rect_to_viewport(Rect2::new(0.0, 0.0, out_w as f32, out_h as f32));
        let show_checkerboard = cx
            .gds()
            .preferences()
            .service()
            .read(cx)
            .current()
            .checkerboard_backdrop;

        let checker_layer = show_checkerboard.then(|| render_checkerboard(img_vp, vp_size, cx));
        let image_layer = doc
            .cache
            .presented_output()
            .map(|out| render_document_output(out, tf, vp_size, img_vp));
        let overlay_layer = self.render_interactive_overlays(vp_size, cx);
        let staged_crop_bar = self.render_staged_crop_bar(vp_size, cx);
        let pending_text = self.render_pending_text_input(vp_size, cx);
        let loading_overlay = self
            .runtime
            .is_loading
            .then(|| self.render_loading_pill(cx));

        let cursor_style = self.viewport_cursor_style();
        let cell_down_l = vp_origin_cell.clone();
        let cell_down_m = vp_origin_cell.clone();
        let cell_move = vp_origin_cell.clone();
        let cell_up_l = vp_origin_cell.clone();
        let cell_up_l_out = vp_origin_cell.clone();
        let cell_up_m = vp_origin_cell.clone();
        let cell_up_m_out = vp_origin_cell.clone();
        let cell_wheel = vp_origin_cell.clone();

        el! {
            .div {
                # "images-viewport";
                test_support;
                role: gpui_kit::Role::Image;
                aria_label: accessible_name;
                key_context: CANVAS_CONTEXT;
                track_focus: $(&self.focus.canvas);
                tab_index: 0;
                size_full;
                relative;
                overflow_hidden;
                cursor: cursor_style;
                on_drop: $(cx.listener(Self::on_external_drop));
                drag_over: $(|style, _: &ExternalPaths, _, cx| style.bg(cx.theme().drop_target));
                on_mouse_down: MouseButton::Left, $(cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    this.on_canvas_mouse_down(ev, cell_down_l.get(), window, cx);
                }));
                on_mouse_down: MouseButton::Middle, $(cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    this.on_canvas_mouse_down(ev, cell_down_m.get(), window, cx);
                }));
                on_mouse_move: $(cx.listener(move |this, ev: &MouseMoveEvent, window, cx| {
                    this.on_canvas_mouse_move(ev, cell_move.get(), window, cx);
                }));
                on_mouse_up: MouseButton::Left, $(cx.listener(move |this, ev: &MouseUpEvent, window, cx| {
                    this.on_canvas_mouse_up(ev, cell_up_l.get(), window, cx);
                }));
                on_mouse_up_out: MouseButton::Left, $(cx.listener(move |this, ev: &MouseUpEvent, window, cx| {
                    this.on_canvas_mouse_up(ev, cell_up_l_out.get(), window, cx);
                }));
                on_mouse_up: MouseButton::Middle, $(cx.listener(move |this, ev: &MouseUpEvent, window, cx| {
                    this.on_canvas_mouse_up(ev, cell_up_m.get(), window, cx);
                }));
                on_mouse_up_out: MouseButton::Middle, $(cx.listener(move |this, ev: &MouseUpEvent, window, cx| {
                    this.on_canvas_mouse_up(ev, cell_up_m_out.get(), window, cx);
                }));
                on_scroll: $(cx.listener(move |this, ev: &ScrollEvent, _, cx| {
                    this.on_canvas_wheel(ev, cell_wheel.get(), cx);
                }));
            } [
                origin_tracker,
                @if (let Some(layer) = checker_layer) layer, @end,
                @if (let Some(layer) = image_layer) layer, @end,
                overlay_layer,
                @if (let Some(bar) = staged_crop_bar) bar, @end,
                @if (let Some(input) = pending_text) input, @end,
                @if (let Some(pill) = loading_overlay) pill, @end,
            ]
        }
        .into_any_element()
    }

    pub(in crate::pages::workspace::ui) fn viewport_cursor_style(&self) -> CursorStyle {
        if self.canvas.panning {
            return CursorStyle::ClosedHand;
        }
        if self.canvas.tool_fsm.tool == ToolMode::Pan {
            return CursorStyle::OpenHand;
        }
        match &self.canvas.tool_fsm.interaction {
            ActiveInteraction::Moving { .. } | ActiveInteraction::DuplicatingMove { .. } => {
                CursorStyle::ClosedHand
            }
            ActiveInteraction::Resizing { handle, .. } => cursor_for_handle(*handle),
            ActiveInteraction::Marquee { .. } => CursorStyle::Arrow,
            ActiveInteraction::Creating { .. } => CursorStyle::Crosshair,
            ActiveInteraction::EditingText(_) => CursorStyle::IBeam,
            ActiveInteraction::Idle | ActiveInteraction::PendingClickOrMove { .. } => {
                match self.canvas.hover_target {
                    HoverTarget::Handle { handle, .. } => cursor_for_handle(handle),
                    HoverTarget::Body { .. } => CursorStyle::PointingHand,
                    HoverTarget::None => match self.canvas.tool_fsm.tool {
                        ToolMode::Pointer => CursorStyle::Arrow,
                        ToolMode::Text => CursorStyle::IBeam,
                        ToolMode::Pan => CursorStyle::OpenHand,
                        _ => CursorStyle::Crosshair,
                    },
                }
            }
        }
    }

    fn render_loading_preview(&self, cx: &App) -> impl IntoElement + use<> {
        let cached_thumb = self
            .runtime
            .loading_origin
            .as_ref()
            .and_then(|origin| match origin {
                Origin::LocalFile { path, .. } => {
                    let meta = self.session.catalog.meta_for_path(path)?;
                    let source = entry_inspection_source(
                        &crate::entities::catalog::CatalogEntry::new(path.clone(), meta.clone()),
                    );
                    let snap = self.runtime.inspections.read(cx).get(&source)?;
                    if let InspectionState::Ready(image) = &snap.thumbnail {
                        Some(Arc::clone(image))
                    } else {
                        None
                    }
                }
                Origin::Memory { .. } => None,
            });

        el! {
            .div {
                flex; flex_col; items_center; justify_center; gap_3; size_full; relative;
            } [
                @if (let Some(image) = cached_thumb)
                    $(img(image).object_fit(ObjectFit::Contain).max_w(px(420.0)).max_h(px(420.0))),
                @end,
                $(self.render_loading_pill(cx)),
            ]
        }
    }

    fn render_loading_pill(&self, cx: &App) -> impl IntoElement + use<> {
        let label = text("loading-image", cx);
        el! {
            .div {
                absolute;
                bottom_4;
                left_4;
                flex;
                items_center;
                gap_2;
                px_3;
                py_1p5;
                rounded_md;
                border_1;
                border_color: $(cx.theme().border);
                bg: $(cx.theme().popover);
                text_xs;
                text_color: $(cx.theme().foreground);
            } [
                $(Spinner::new().small()),
                label,
            ]
        }
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let offer = (!self.session.clipboard_dismissed)
            .then_some(self.runtime.clipboard_offer.as_ref())
            .flatten();

        let banner = offer.map(|kind| {
            let msg = match kind {
                ClipboardOfferKind::LocalImageFile(path) => format!(
                    "{} {}",
                    text("clipboard-banner-file", cx),
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string())
                ),
                ClipboardOfferKind::RawImageMime(_) => text("clipboard-banner-image", cx).to_string(),
            };
            el! {
                .div {
                    # "clipboard-offer-banner";
                    flex; items_center; gap_3; px_4; py_2;
                    rounded_lg; border_1; border_color: $(cx.theme().border);
                    bg: $(cx.theme().popover);
                    mb_6;
                } [
                    $(Icon::new(IconName::Clipboard).size(px(18.0)).text_color(cx.theme().primary)),
                    .div { text_sm; text_color: $(cx.theme().foreground); } [msg],
                    "banner-open-clipboard".Button::new {
                        primary;
                        small;
                        label: $(text("clipboard-banner-open", cx));
                        on_click: $(cx.listener(|this, _, window, cx| this.open_from_clipboard(window, cx)));
                    },
                    "banner-dismiss".Button::icon_button {
                        control_icon: IconName::X;
                        tooltip: $(text("close", cx));
                        on_click: $(cx.listener(|this, _, _, cx| {
                            this.session.clipboard_dismissed = true;
                            cx.notify();
                        }));
                    },
                ]
            }
        });

        el! {
            .div {
                # "images-empty-state";
                size_full;
                flex;
                flex_col;
                items_center;
                justify_center;
                gap_3;
                p_6;
            } [
                @if (let Some(banner) = banner) banner, @end,
                $(Icon::new(IconName::Image).size(px(48.0)).text_color(cx.theme().muted_foreground)),
                .div {
                    text_base; font_semibold; text_color: $(cx.theme().foreground);
                } [$(text("empty-title", cx))],
                .div {
                    text_sm; text_color: $(cx.theme().muted_foreground); text_center; max_w: $(px(360.0));
                } [$(text("empty-subtitle", cx))],
                .div { flex; items_center; gap_2; pt_2; } [
                    "empty-open".Button::new {
                        primary;
                        label: $(text("action-open", cx));
                        on_click: $(cx.listener(|this, _, window, cx| this.open_file_picker(window, cx)));
                    },
                    "empty-clipboard".Button::new {
                        label: $(text("action-open-clipboard", cx));
                        on_click: $(cx.listener(|this, _, window, cx| this.open_from_clipboard(window, cx)));
                    },
                ],
            ]
        }
    }

    fn render_interactive_overlays(&self, vp_size: Size2, cx: &App) -> impl IntoElement + use<> {
        let tf = self.canvas.transform;
        let staged_crop = self.canvas.tool_fsm.staged_crop;
        let active_crop = match &self.canvas.tool_fsm.interaction {
            ActiveInteraction::Creating {
                tool: ToolMode::Crop,
                start_img,
                current_img,
                ..
            } => Some(ViewportTransform::normalize_drag_rect(
                *start_img,
                *current_img,
            )),
            _ => None,
        };
        let marquee_rect = match &self.canvas.tool_fsm.interaction {
            ActiveInteraction::Marquee {
                start_img,
                current_img,
                ..
            } => Some(ViewportTransform::normalize_drag_rect(
                *start_img,
                *current_img,
            )),
            _ => None,
        };

        // Sync selection outlines & handles strictly to the currently presented GPU texture.
        let presented_items: Vec<AnnotationItem> = self
            .session
            .document
            .as_ref()
            .map(|doc| doc.cache.presented_annotations(&doc.annotations).to_vec())
            .unwrap_or_default();

        let selection = self.canvas.tool_fsm.selection.clone();
        let hover_discovery_bounds =
            if matches!(self.canvas.tool_fsm.interaction, ActiveInteraction::Idle)
                && let HoverTarget::Body {
                    id,
                    discover_redaction: true,
                } = self.canvas.hover_target
            {
                presented_items
                    .iter()
                    .find(|item| item.id == id)
                    .map(AnnotationItem::visual_bounds)
            } else {
                None
            };

        let show_selection_chrome = staged_crop.is_none() && active_crop.is_none();
        let selected_items: Vec<AnnotationItem> = if show_selection_chrome {
            selection.selected_items_ordered(&presented_items)
        } else {
            Vec::new()
        };
        let union_bounds = (selected_items.len() >= 2)
            .then(|| selection.union_bounds(&presented_items))
            .flatten();

        let primary_color = cx.theme().primary;
        let handle_fill: Hsla = gpui_kit::rgba(0xffffffff).into();
        let scrim: Hsla = gpui_kit::rgba(0x00000088).into();

        canvas(
            move |_, _, _| {},
            move |bounds, _, window, _| {
                let ox = f32::from(bounds.origin.x);
                let oy = f32::from(bounds.origin.y);

                if let Some(crop) = staged_crop {
                    let vp = tf.image_rect_to_viewport(Rect2::new(
                        crop.x as f32,
                        crop.y as f32,
                        crop.width as f32,
                        crop.height as f32,
                    ));
                    paint_crop_overlay(ox, oy, vp, vp_size, scrim, primary_color, window);
                } else if let Some(crop_drag) = active_crop {
                    let vp = tf.image_rect_to_viewport(crop_drag);
                    paint_crop_overlay(ox, oy, vp, vp_size, scrim, primary_color, window);
                }

                if let Some(redaction_bounds) = hover_discovery_bounds {
                    let vp = tf.image_rect_to_viewport(redaction_bounds);
                    paint_dashed_rect(ox, oy, vp, primary_color.opacity(0.85), window);
                }

                if selected_items.len() == 1 {
                    let item = &selected_items[0];
                    paint_single_selection(ox, oy, item, tf, primary_color, handle_fill, window);
                } else if selected_items.len() >= 2 {
                    for item in &selected_items {
                        let vp = tf.image_rect_to_viewport(item.visual_bounds());
                        paint_rect_outline(ox, oy, vp, primary_color.opacity(0.65), 1.0, window);
                    }
                    if let Some(union) = union_bounds {
                        let vp = tf.image_rect_to_viewport(union);
                        paint_dashed_rect(ox, oy, vp, primary_color, window);
                    }
                }

                if let Some(marquee) = marquee_rect {
                    let vp = tf.image_rect_to_viewport(marquee);
                    let fill_rect = Bounds::new(
                        point(px(ox + vp.x), px(oy + vp.y)),
                        size(px(vp.width), px(vp.height)),
                    );
                    window.paint_quad(fill(fill_rect, primary_color.opacity(0.14)));
                    paint_dashed_rect(ox, oy, vp, primary_color, window);
                }
            },
        )
        .absolute()
        .inset_0()
    }

    fn render_staged_crop_bar(
        &self,
        vp_size: Size2,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let crop = self.canvas.tool_fsm.staged_crop?;
        let vp = self.canvas.transform.image_rect_to_viewport(Rect2::new(
            crop.x as f32,
            crop.y as f32,
            crop.width as f32,
            crop.height as f32,
        ));
        let left = vp.x.clamp(8.0, (vp_size.width - 210.0).max(8.0));
        // Flip the crop confirmation bar inside the box when too close to the viewport top edge.
        let top = if vp.y < 38.0 {
            (vp.y + 6.0).clamp(6.0, (vp_size.height - 40.0).max(6.0))
        } else {
            (vp.y - 36.0).max(6.0)
        };

        Some(el! {
            .div {
                # "staged-crop-bar";
                absolute;
                left: $(px(left));
                top: $(px(top));
                flex;
                items_center;
                gap_1p5;
                p_1;
                rounded_md;
                border_1;
                border_color: $(cx.theme().border);
                bg: $(cx.theme().popover);
                shadow_md;
            } [
                "crop-apply-btn".Button::new {
                    primary;
                    small;
                    icon: IconName::Check;
                    label: $(text("tool-apply-crop", cx));
                    on_click: $(cx.listener(|this, _, window, cx| this.apply_staged_crop(window, cx)));
                },
                "crop-clear-btn".Button::new {
                    ghost;
                    small;
                    icon: IconName::X;
                    label: $(text("tool-clear-crop", cx));
                    on_click: $(cx.listener(|this, _, _, cx| this.clear_staged_crop(cx)));
                },
            ]
        })
    }

    fn render_pending_text_input(
        &self,
        vp_size: Size2,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let ActiveInteraction::EditingText(draft) = &self.canvas.tool_fsm.interaction else {
            return None;
        };
        let img_pos = match &draft.target {
            TextEditTarget::New { position } => *position,
            TextEditTarget::Existing { original, .. } => {
                let bounds = original.visual_bounds();
                Point2::new(bounds.x, bounds.y)
            }
        };
        let vp = self.canvas.transform.image_to_viewport(img_pos);
        let left = vp.x.clamp(8.0, (vp_size.width - 332.0).max(8.0));
        let top = vp.y.clamp(8.0, (vp_size.height - 72.0).max(8.0));

        Some(el! {
            .div {
                # "text-overlay-box";
                test_support;
                absolute;
                left: $(px(left));
                top: $(px(top));
                flex;
                items_start;
                gap_1;
                p_1;
                rounded_md;
                border_1;
                border_color: $(cx.theme().border);
                bg: $(cx.theme().popover);
                shadow_md;
                on_mouse_down: MouseButton::Left, $(|_, _, cx| cx.stop_propagation());
            } [
                .div { w: $(px(240.0)); } [
                    $(Textarea::new(&self.focus.text_overlay_input).small().w_full()),
                ],
                "text-commit-btn".Button::icon_button {
                    control_icon: IconName::Check;
                    tooltip: $(text("tool-commit-text", cx));
                    on_click: $(cx.listener(|this, _, window, cx| this.commit_text_overlay(window, cx)));
                },
                "text-cancel-btn".Button::icon_button {
                    control_icon: IconName::X;
                    tooltip: $(text("tool-cancel-text", cx));
                    on_click: $(cx.listener(|this, _, window, cx| this.cancel_text_overlay(window, cx)));
                },
            ]
        })
    }

    fn on_external_drop(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(first) = paths.paths().first().cloned() else {
            return;
        };
        self.open_local_with_confirm(first, window, cx);
    }

    fn on_canvas_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        vp_origin: Point<gpui_kit::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.tool_fsm.is_editing_text() {
            self.flush_editing_text_before_external(window, cx);
        }
        self.focus.canvas.focus(window, cx);
        let vp_pt = Point2::new(
            f32::from(event.position.x - vp_origin.x),
            f32::from(event.position.y - vp_origin.y),
        );

        if event.button == MouseButton::Middle || self.canvas.tool_fsm.tool == ToolMode::Pan {
            self.canvas.panning = true;
            self.canvas.last_pointer = Some(vp_pt);
            cx.notify();
            return;
        }
        if event.button != MouseButton::Left {
            return;
        }

        let Some(doc) = &self.session.document else {
            return;
        };
        let img_pt = self.canvas.transform.viewport_to_image(vp_pt);
        let input = PointerDownInput {
            vp_pt,
            img_pt,
            scale: self.canvas.transform.scale,
            modifiers: to_pointer_modifiers(event.modifiers),
            click_count: event.click_count,
        };
        let outcome = self
            .canvas
            .tool_fsm
            .on_pointer_down(doc, &mut self.canvas.history, input);

        match outcome {
            PointerDownOutcome::OpenTextEditor { initial_text } => {
                let has_initial = !initial_text.trim().is_empty();
                self.open_text_editor_with_value(initial_text, window, cx);
                if has_initial {
                    self.schedule_gesture_preview(cx);
                }
            }
            PointerDownOutcome::NeedsTransientPreview => {
                self.schedule_gesture_preview(cx);
            }
            PointerDownOutcome::SelectionChanged | PointerDownOutcome::None => {}
        }
        cx.notify();
    }

    fn on_canvas_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        vp_origin: Point<gpui_kit::Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let vp_pt = Point2::new(
            f32::from(event.position.x - vp_origin.x),
            f32::from(event.position.y - vp_origin.y),
        );
        if self.canvas.panning {
            if let Some(prev) = self.canvas.last_pointer {
                self.canvas
                    .transform
                    .pan_by(vp_pt.x - prev.x, vp_pt.y - prev.y);
            }
            self.canvas.last_pointer = Some(vp_pt);
            cx.notify();
            return;
        }

        let Some(doc) = &self.session.document else {
            return;
        };
        let img_pt = self.canvas.transform.viewport_to_image(vp_pt);
        if self.canvas.tool_fsm.is_pointer_gesture() {
            match self.canvas.tool_fsm.on_pointer_move(
                doc,
                vp_pt,
                img_pt,
                to_pointer_modifiers(event.modifiers),
            ) {
                PointerMoveOutcome::NeedsTransientPreview { .. } => {
                    self.schedule_gesture_preview(cx);
                    cx.notify();
                }
                PointerMoveOutcome::ChromeOnly => {
                    cx.notify();
                }
                PointerMoveOutcome::None => {}
            }
            return;
        }

        if matches!(self.canvas.tool_fsm.interaction, ActiveInteraction::Idle) {
            let annotations = doc.cache.presented_annotations(&doc.annotations);
            let next_hover = hover_test_canvas(
                annotations,
                &self.canvas.tool_fsm.selection,
                img_pt,
                self.canvas.tool_fsm.tool,
                self.canvas.transform.scale,
            );
            if next_hover != self.canvas.hover_target {
                self.canvas.hover_target = next_hover;
                cx.notify();
            }
        }
    }

    fn on_canvas_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _: Point<gpui_kit::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Middle || self.canvas.panning {
            self.canvas.panning = false;
            self.canvas.last_pointer = None;
            cx.notify();
            return;
        }
        if event.button != MouseButton::Left || !self.canvas.tool_fsm.is_pointer_gesture() {
            return;
        }
        let scale = self.canvas.transform.scale;
        let Some(doc) = &mut self.session.document else {
            self.canvas
                .tool_fsm
                .cancel_pointer_gesture(&mut self.canvas.history);
            self.cancel_gesture_render();
            return;
        };
        let outcome = self
            .canvas
            .tool_fsm
            .on_pointer_up(doc, &mut self.canvas.history, scale);
        match outcome {
            FinishOutcome::Committed => {
                // Keep the latest transient scene visible until `Complete(R+1)` installs.
                self.sync_window_title(window, cx);
                self.rebuild_textures(cx);
            }
            FinishOutcome::SelectionChanged | FinishOutcome::CropStaged(_) => {
                self.cancel_gesture_render();
            }
            FinishOutcome::None => {
                self.restore_complete_presentation(cx);
            }
        }
        cx.notify();
    }

    fn on_canvas_wheel(
        &mut self,
        event: &ScrollEvent,
        vp_origin: Point<gpui_kit::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.session.document.is_none() {
            return;
        }
        let delta_y = match event.delta {
            ScrollDelta::Pixels(p) => f32::from(p.y) / 120.0,
            ScrollDelta::Lines(l) => l.y * 0.25,
        };
        if !delta_y.is_finite() || delta_y == 0.0 {
            return;
        }
        let factor = (1.12_f32).powf(delta_y);
        let cursor = Point2::new(
            f32::from(event.position.x - vp_origin.x),
            f32::from(event.position.y - vp_origin.y),
        );
        self.canvas.transform.zoom_around_cursor(factor, cursor);
        self.schedule_svg_rerender_if_needed(cx);
        cx.notify();
    }
}

const fn to_pointer_modifiers(mods: Modifiers) -> PointerModifiers {
    PointerModifiers {
        shift: mods.shift,
        ctrl: mods.control || mods.platform,
        alt: mods.alt,
    }
}

const fn cursor_for_handle(handle: HandleKind) -> CursorStyle {
    // Note `gpui-fast` swaps diagonal cursor names relative to CSS conventions:
    // `ResizeUpRightDownLeft` maps to `nwse-resize`, `ResizeUpLeftDownRight` to `nesw-resize`.
    match handle {
        HandleKind::Nw | HandleKind::Se => CursorStyle::ResizeUpRightDownLeft,
        HandleKind::Ne | HandleKind::Sw => CursorStyle::ResizeUpLeftDownRight,
        HandleKind::N | HandleKind::S => CursorStyle::ResizeUpDown,
        HandleKind::E | HandleKind::W => CursorStyle::ResizeLeftRight,
        HandleKind::LineStart | HandleKind::LineEnd => CursorStyle::Crosshair,
    }
}

fn render_checkerboard(img_vp: Rect2, vp_size: Size2, cx: &App) -> impl IntoElement + use<> {
    let light = gpui_kit::rgba(0xe5e7ebff);
    let dark = gpui_kit::rgba(0xd1d5dbff);
    let dark_mode = cx.theme().is_dark();
    let (c0, c1) = if dark_mode {
        (gpui_kit::rgba(0x27272aff), gpui_kit::rgba(0x1f1f23ff))
    } else {
        (light, dark)
    };

    canvas(
        move |_, _, _| {},
        move |bounds, _, window, _| {
            let clip_x0 = img_vp.x.max(0.0);
            let clip_y0 = img_vp.y.max(0.0);
            let clip_x1 = (img_vp.x + img_vp.width).min(vp_size.width);
            let clip_y1 = (img_vp.y + img_vp.height).min(vp_size.height);
            if clip_x1 <= clip_x0 || clip_y1 <= clip_y0 {
                return;
            }

            let start_col = ((clip_x0 - img_vp.x) / CHECKER_TILE).floor().max(0.0) as i32;
            let end_col = ((clip_x1 - img_vp.x) / CHECKER_TILE).ceil().max(0.0) as i32;
            let start_row = ((clip_y0 - img_vp.y) / CHECKER_TILE).floor().max(0.0) as i32;
            let end_row = ((clip_y1 - img_vp.y) / CHECKER_TILE).ceil().max(0.0) as i32;

            let ox = f32::from(bounds.origin.x);
            let oy = f32::from(bounds.origin.y);

            for row in start_row..end_row {
                let y0 = (img_vp.y + row as f32 * CHECKER_TILE).max(clip_y0);
                let y1 = (img_vp.y + (row + 1) as f32 * CHECKER_TILE).min(clip_y1);
                if y1 <= y0 {
                    continue;
                }
                for col in start_col..end_col {
                    let x0 = (img_vp.x + col as f32 * CHECKER_TILE).max(clip_x0);
                    let x1 = (img_vp.x + (col + 1) as f32 * CHECKER_TILE).min(clip_x1);
                    if x1 <= x0 {
                        continue;
                    }
                    let color = if (row + col) % 2 == 0 { c0 } else { c1 };
                    let rect = Bounds::new(
                        point(px(ox + x0), px(oy + y0)),
                        size(px(x1 - x0), px(y1 - y0)),
                    );
                    window.paint_quad(fill(rect, color));
                }
            }
        },
    )
    .absolute()
    .inset_0()
}

fn render_document_output(
    output: &RenderOutput,
    tf: ViewportTransform,
    vp_size: Size2,
    img_vp: Rect2,
) -> impl IntoElement + use<> {
    match output {
        RenderOutput::Single { image } => el! {
            .div {
                absolute;
                left: $(px(img_vp.x));
                top: $(px(img_vp.y));
                w: $(px(img_vp.width));
                h: $(px(img_vp.height));
            } [
                $(img(Arc::clone(image)).object_fit(ObjectFit::Fill).size_full()),
            ]
        }
        .into_any_element(),
        RenderOutput::Tiled { tiles } => {
            let visible_tiles = tiles
                .iter()
                .filter_map(|(spec, image)| {
                    let vp = tf.image_rect_to_viewport(Rect2::new(
                        spec.x as f32,
                        spec.y as f32,
                        spec.width as f32,
                        spec.height as f32,
                    ));
                    let visible = vp.x + vp.width > 0.0
                        && vp.y + vp.height > 0.0
                        && vp.x < vp_size.width
                        && vp.y < vp_size.height;
                    visible.then(|| {
                        el! {
                            .div {
                                absolute;
                                left: $(px(vp.x));
                                top: $(px(vp.y));
                                w: $(px(vp.width));
                                h: $(px(vp.height));
                            } [
                                $(img(Arc::clone(image)).object_fit(ObjectFit::Fill).size_full()),
                            ]
                        }
                    })
                })
                .collect::<Vec<_>>();
            el! { .div { absolute; inset_0; } [..visible_tiles] }.into_any_element()
        }
    }
}

fn paint_crop_overlay(
    ox: f32,
    oy: f32,
    vp: Rect2,
    vp_size: Size2,
    scrim: Hsla,
    border: Hsla,
    window: &mut Window,
) {
    let x0 = vp.x.clamp(0.0, vp_size.width);
    let y0 = vp.y.clamp(0.0, vp_size.height);
    let x1 = (vp.x + vp.width).clamp(0.0, vp_size.width);
    let y1 = (vp.y + vp.height).clamp(0.0, vp_size.height);

    let bands = [
        (0.0, 0.0, vp_size.width, y0),
        (0.0, y1, vp_size.width, vp_size.height - y1),
        (0.0, y0, x0, y1 - y0),
        (x1, y0, vp_size.width - x1, y1 - y0),
    ];
    for (bx, by, bw, bh) in bands {
        if bw > 0.0 && bh > 0.0 {
            window.paint_quad(fill(
                Bounds::new(point(px(ox + bx), px(oy + by)), size(px(bw), px(bh))),
                scrim,
            ));
        }
    }
    paint_rect_outline(ox, oy, vp, border, 2.0, window);
}

fn paint_rect_outline(
    ox: f32,
    oy: f32,
    rect: Rect2,
    color: Hsla,
    thickness: f32,
    window: &mut Window,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let edges = [
        (rect.x, rect.y, rect.width, thickness),
        (
            rect.x,
            rect.y + rect.height - thickness,
            rect.width,
            thickness,
        ),
        (rect.x, rect.y, thickness, rect.height),
        (
            rect.x + rect.width - thickness,
            rect.y,
            thickness,
            rect.height,
        ),
    ];
    for (ex, ey, ew, eh) in edges {
        window.paint_quad(fill(
            Bounds::new(point(px(ox + ex), px(oy + ey)), size(px(ew), px(eh))),
            color,
        ));
    }
}

fn paint_dashed_rect(ox: f32, oy: f32, rect: Rect2, color: Hsla, window: &mut Window) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let bounds = Bounds::new(
        point(px(ox + rect.x), px(oy + rect.y)),
        size(px(rect.width), px(rect.height)),
    );
    window.paint_quad(outline(bounds, color, BorderStyle::Dashed));
}

fn paint_single_selection(
    ox: f32,
    oy: f32,
    item: &AnnotationItem,
    tf: ViewportTransform,
    primary_color: Hsla,
    handle_fill: Hsla,
    window: &mut Window,
) {
    match &item.kind {
        crate::entities::document::AnnotationKind::Arrow { start, end, .. }
        | crate::entities::document::AnnotationKind::Line { start, end, .. } => {
            let p0 = tf.image_to_viewport(Point2::new(start[0], start[1]));
            let p1 = tf.image_to_viewport(Point2::new(end[0], end[1]));
            let mut builder = PathBuilder::stroke(px(1.5)).dash_array(&[px(4.0), px(4.0)]);
            builder.move_to(point(px(ox + p0.x), px(oy + p0.y)));
            builder.line_to(point(px(ox + p1.x), px(oy + p1.y)));
            if let Ok(path) = builder.build() {
                window.paint_path(path, primary_color.opacity(0.8));
            }
        }
        _ => {
            let vp = tf.image_rect_to_viewport(item.visual_bounds());
            paint_rect_outline(ox, oy, vp, primary_color.opacity(0.85), 1.0, window);
        }
    }

    let half = HANDLE_DRAW_SIZE * 0.5;
    for (_, img_pt) in handles_for_item(item) {
        let vp = tf.image_to_viewport(img_pt);
        let rect = Bounds::new(
            point(px(ox + vp.x - half), px(oy + vp.y - half)),
            size(px(HANDLE_DRAW_SIZE), px(HANDLE_DRAW_SIZE)),
        );
        let mut quad = fill(rect, handle_fill);
        quad.border_widths = (1.5).into();
        quad.border_color = primary_color;
        window.paint_quad(quad);
    }
}
