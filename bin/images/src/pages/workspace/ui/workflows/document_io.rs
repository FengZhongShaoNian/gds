//! Image decoding, preview fallback, complete/transient texture installation, latest-wins
//! gesture scheduling, SVG zoom rerender, and discard guard.

use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
};

use gds::{
    i18n::t,
    service::image::{
        Error as ImageError,
        decode::{DecodeLimits, decode_origin, decode_preview, render_svg_scaled},
        source::{Format, Origin},
    },
    ui::{DialogActions, notifications::Level},
};
use gds_uim::el;
use gpui_kit::{
    Context, Window,
    assets::IconName,
    component::{Icon, WindowExt as _, dialog::AlertDialog},
};
use image::RgbaImage;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{
    entities::{
        document::{
            AnnotationItem, DocumentState, GesturePrepareInput, GestureSession, PreparedPayload,
            PreparedTextures, RenderCache, SceneTag, TransformState, TransientFrame,
            build_render_textures, build_svg_zoom_textures, encode_display_payload,
        },
        metadata::{self, MetadataState},
    },
    features::{edit_canvas::HoverTarget, navigation::open_file_dialog},
    shared::{
        clipboard::ClipboardExt as _,
        geometry::{FitMode, Size2, ViewportTransform},
        i18n::text,
    },
};

const PREVIEW_FALLBACK_MAX_EDGE: u32 = 2048;

#[derive(Clone)]
pub(in crate::pages::workspace::ui) struct GesturePrepareSeed {
    pub base_rgba: Arc<RgbaImage>,
    pub transform: TransformState,
    pub cached_base: Option<(TransformState, Arc<RgbaImage>)>,
    pub complete_rgba: Option<Arc<RgbaImage>>,
    pub initial_annotations: Arc<[AnnotationItem]>,
}

pub(in crate::pages::workspace::ui) struct GestureRenderHandle {
    pub gesture_id: u64,
    pub base_revision: u64,
    pub first_changed_idx: usize,
    pub next_seq: u64,
    pub in_flight: bool,
    pub pending: Option<(u64, Arc<[AnnotationItem]>)>,
    pub cancel: CancellationToken,
    pub prepare_seed: GesturePrepareSeed,
    pub session: Arc<Mutex<Option<GestureSession>>>,
}

struct DecodeResultPayload {
    decoded: gds::service::image::decode::DecodedImage,
    tags: Vec<String>,
    fallback_error: Option<String>,
}

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn confirm_discard_changes(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        on_confirm: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.flush_editing_text_before_external(window, cx);
        self.cancel_pointer_gesture_before_external(cx);

        if !self.has_unsaved_changes() {
            on_confirm(self, window, cx);
            return;
        }
        if window.has_active_dialog(cx) {
            return;
        }
        let callback = Rc::new(Cell::new(Some(on_confirm)));
        let page = cx.entity().downgrade();
        let title = text("dialog-discard-title", cx);
        let description = text("dialog-discard-changes", cx);
        let confirm_label = text("dialog-discard-confirm", cx);
        let cancel_label = text("dialog-discard-cancel", cx);

        window.open_alert_dialog(cx, move |dialog: AlertDialog, _, _| {
            let page = page.clone();
            let callback = callback.clone();
            let footer = el! {
                .DialogActions::default {
                    danger;
                    primary_label: $(confirm_label.clone());
                    primary_icon: IconName::Check;
                    cancel_label: $(cancel_label.clone());
                }
            };
            el! {
                dialog {
                    confirm;
                    title: $(title.clone());
                    description: $(description.clone());
                    icon: $(Icon::new(IconName::Info));
                    footer: footer;
                    on_ok: $(move |_, window, cx| {
                        if let Some(action) = callback.take() {
                            let _ = page.update(cx, |this, cx| action(this, window, cx));
                        }
                        true
                    });
                }
            }
        });
    }

    pub(in crate::pages::workspace::ui) fn open_file_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = open_file_dialog(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(path) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.open_local_with_confirm(path, window, cx);
            });
        })
        .detach();
    }

    pub(in crate::pages::workspace::ui) fn open_from_clipboard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_discard_changes(window, cx, |_this, window, cx| {
            let task = cx.gds().clipboard().read_origin();
            cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                let _ = this.update_in(cx, |this, window, cx| match result {
                    Ok(origin) => this.open_origin(origin, window, cx),
                    Err(err) => {
                        show_report::<LoadNotice>(
                            Level::Error,
                            t!(@cx, "toast-clipboard-open-failed", error = err.to_string()),
                            cx,
                        );
                    }
                });
            })
            .detach();
        });
    }

    pub(in crate::pages::workspace::ui) fn open_local_path(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_origin(Origin::LocalFile { path, stamp: None }, window, cx);
    }

    pub(in crate::pages::workspace::ui) fn open_origin(
        &mut self,
        origin: Origin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(cancel) = self.session.load_cancel.take() {
            cancel.cancel();
        }
        if let Some(cancel) = self.canvas.render_cancel.take() {
            cancel.cancel();
        }
        self.cancel_gesture_render();
        self.cancel_svg_rerender();

        self.session.load_generation = self.session.load_generation.wrapping_add(1);
        let load_gen = self.session.load_generation;
        let cancel = CancellationToken::new();
        self.session.load_cancel = Some(cancel.clone());
        self.runtime.is_loading = true;
        self.runtime.load_error = None;
        self.runtime.loading_origin = Some(origin.clone());

        if let Origin::LocalFile { path, .. } = &origin {
            self.session.catalog.select_path(path);
            self.prefetch_neighbor_thumbnails(cx);
        }
        cx.notify();

        let decode_origin_val = origin.clone();
        let task = self.tasks.spawn_blocking_named("Decode image", move || {
            let (decoded, fallback_error) =
                match decode_origin(&decode_origin_val, DecodeLimits::default(), &cancel) {
                    Ok(decoded) => (decoded, None),
                    Err(
                        err @ (ImageError::TooLarge { .. } | ImageError::DimensionsTooLarge { .. }),
                    ) => {
                        let fallback_msg = err.to_string();
                        let preview = decode_preview(
                            &decode_origin_val,
                            PREVIEW_FALLBACK_MAX_EDGE,
                            DecodeLimits::default(),
                            &cancel,
                        )?;
                        (preview, Some(fallback_msg))
                    }
                    Err(err) => return Err(err),
                };

            let tags = match &decoded.origin {
                Origin::LocalFile { path, .. } => metadata::load_tags(path),
                Origin::Memory { .. } => Vec::new(),
            };
            Ok(DecodeResultPayload {
                decoded,
                tags,
                fallback_error,
            })
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.session.load_generation != load_gen {
                    return;
                }
                this.session.load_cancel = None;
                this.runtime.is_loading = false;
                this.runtime.loading_origin = None;

                match result {
                    Ok(Ok(payload)) => {
                        this.finish_document_load(payload, window, cx);
                    }
                    Ok(Err(ImageError::Cancelled)) => {
                        cx.notify();
                    }
                    Ok(Err(err)) => {
                        let msg = err.to_string();
                        this.runtime.load_error = Some(msg.clone());
                        show_report::<LoadNotice>(
                            Level::Error,
                            t!(@cx, "toast-load-failed", error = msg),
                            cx,
                        );
                        cx.notify();
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        this.runtime.load_error = Some(msg.clone());
                        show_report::<LoadNotice>(
                            Level::Error,
                            t!(@cx, "toast-load-failed", error = msg),
                            cx,
                        );
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn finish_document_load(
        &mut self,
        payload: DecodeResultPayload,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(old_doc) = self.session.document.as_mut() {
            old_doc.cache.release_textures(cx);
        }

        let meta = MetadataState::from_bundle(payload.decoded.metadata.clone(), payload.tags);
        let doc = DocumentState::from_decoded(payload.decoded);
        let (w, h) = doc.output_dimensions();
        let origin = doc.decoded.origin.clone();

        self.session.document = Some(doc);
        self.session.metadata = Some(meta);
        self.session.editing_path = false;
        self.session.clipboard_dismissed = true;
        self.canvas.history.clear();
        self.canvas.tool_fsm.selection.clear();
        self.canvas.tool_fsm.cancel_active(&mut self.canvas.history);
        self.canvas.hover_target = HoverTarget::None;
        self.cancel_gesture_render();
        self.clear_text_overlay_input(window, cx);
        self.canvas.svg_rerender_scale = None;
        self.canvas.transform = ViewportTransform::compute_fit(
            FitMode::FitToWindow,
            self.chrome.last_viewport_size,
            Size2::new(w as f32, h as f32),
        );

        self.sync_window_title(window, cx);
        self.rebuild_textures(cx);

        if let Origin::LocalFile { path, .. } = origin {
            let same_dir = path
                .parent()
                .is_some_and(|dir| self.session.catalog.directory.as_deref() == Some(dir));
            if same_dir && self.session.catalog.select_path(&path) {
                self.prefetch_neighbor_thumbnails(cx);
            } else {
                self.start_sibling_scan(path, cx);
            }
        }

        if let Some(error) = payload.fallback_error {
            show_report::<LoadNotice>(
                Level::Warning,
                t!(@cx, "toast-preview-fallback", error = error),
                cx,
            );
        }
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn cancel_gesture_render(&mut self) {
        if let Some(handle) = self.canvas.gesture_render.take() {
            handle.cancel.cancel();
        }
    }

    pub(in crate::pages::workspace::ui) fn schedule_gesture_preview(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.session.document else {
            return;
        };
        let Some((first_changed_idx, snapshot)) = self.canvas.tool_fsm.transient_annotations(doc)
        else {
            return;
        };

        let edit_rev = doc.edit_revision;
        let reuse = self.canvas.gesture_render.as_ref().is_some_and(|handle| {
            handle.base_revision == edit_rev
                && handle.first_changed_idx == first_changed_idx
                && !handle.cancel.is_cancelled()
        });

        if !reuse {
            let prepare_seed = GesturePrepareSeed {
                base_rgba: Arc::clone(&doc.decoded.rgba),
                transform: doc.transform,
                cached_base: doc.cache.transformed_base.clone(),
                complete_rgba: doc.cache.complete_at(edit_rev).cloned(),
                initial_annotations: Arc::from(doc.annotations.as_slice()),
            };
            self.cancel_gesture_render();
            self.canvas.gesture_generation = self.canvas.gesture_generation.wrapping_add(1);
            let gesture_id = self.canvas.gesture_generation;
            let cancel = CancellationToken::new();
            self.canvas.gesture_render = Some(GestureRenderHandle {
                gesture_id,
                base_revision: edit_rev,
                first_changed_idx,
                next_seq: 0,
                in_flight: false,
                pending: None,
                cancel,
                prepare_seed,
                session: Arc::new(Mutex::new(None)),
            });
        }

        if let Some(handle) = &mut self.canvas.gesture_render {
            handle.next_seq = handle.next_seq.saturating_add(1);
            let seq = handle.next_seq;
            handle.pending = Some((seq, snapshot));
            if !handle.in_flight {
                self.pump_gesture_render(cx);
            }
        }
    }

    fn pump_gesture_render(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = &mut self.canvas.gesture_render else {
            return;
        };
        if handle.in_flight || handle.cancel.is_cancelled() {
            return;
        }
        let Some((seq, snapshot)) = handle.pending.take() else {
            return;
        };
        handle.in_flight = true;

        let gesture_id = handle.gesture_id;
        let first_changed_idx = handle.first_changed_idx;
        let cancel = handle.cancel.clone();
        let seed = handle.prepare_seed.clone();
        let session_arc = Arc::clone(&handle.session);
        let render_gen = self.canvas.render_generation;
        let snap_for_worker = Arc::clone(&snapshot);

        let task = self
            .tasks
            .spawn_blocking_named("Render gesture preview", move || {
                let mut guard = session_arc
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let session = match guard.as_mut() {
                    Some(existing) => existing,
                    None => {
                        let prepared = GestureSession::prepare(
                            GesturePrepareInput {
                                base_rgba: seed.base_rgba,
                                transform: seed.transform,
                                cached_base: seed.cached_base,
                                complete_rgba: seed.complete_rgba,
                                initial_annotations: &seed.initial_annotations,
                                first_changed_idx,
                            },
                            &cancel,
                        )?;
                        guard.insert(prepared)
                    }
                };
                session.step(&snap_for_worker, &cancel)
            });

        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let active_match = this
                    .canvas
                    .gesture_render
                    .as_ref()
                    .is_some_and(|h| h.gesture_id == gesture_id && !h.cancel.is_cancelled());
                if !active_match {
                    return;
                }
                if let Some(h) = &mut this.canvas.gesture_render {
                    h.in_flight = false;
                }

                match result {
                    Ok(Ok(payload)) => {
                        if let Some(doc) = &mut this.session.document {
                            doc.cache.install_transient(
                                TransientFrame {
                                    payload,
                                    gesture_id,
                                    seq,
                                    annotations: snapshot,
                                },
                                render_gen,
                                cx,
                            );
                            cx.notify();
                        }
                    }
                    Ok(Err(ImageError::Cancelled)) => {}
                    Ok(Err(err)) => {
                        tracing::debug!(error = %err, "transient gesture render failed");
                    }
                    Err(err) => {
                        tracing::debug!(error = %err, "transient gesture worker aborted");
                    }
                }

                if this
                    .canvas
                    .gesture_render
                    .as_ref()
                    .is_some_and(|h| h.gesture_id == gesture_id && h.pending.is_some())
                {
                    this.pump_gesture_render(cx);
                }
            });
        })
        .detach();
    }

    fn can_install_complete_presentation(&self, edit_rev: u64) -> bool {
        self.session
            .document
            .as_ref()
            .is_some_and(|doc| doc.edit_revision == edit_rev)
            && self.canvas.gesture_render.is_none()
            && !self.canvas.tool_fsm.is_pointer_gesture()
            && !self.canvas.tool_fsm.is_editing_text()
    }

    pub(in crate::pages::workspace::ui) fn restore_complete_presentation(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.cancel_gesture_render();
        let Some(doc) = &self.session.document else {
            return;
        };
        let edit_rev = doc.edit_revision;
        if doc
            .cache
            .presented
            .as_ref()
            .is_some_and(|scene| scene.tag == SceneTag::Complete { revision: edit_rev })
        {
            cx.notify();
            return;
        }
        let Some(complete_rgba) = doc.cache.complete_at(edit_rev).cloned() else {
            self.rebuild_textures(cx);
            return;
        };

        if let Some(cancel) = self.canvas.render_cancel.take() {
            cancel.cancel();
        }
        self.cancel_svg_rerender();
        self.canvas.render_generation = self.canvas.render_generation.wrapping_add(1);
        let render_gen = self.canvas.render_generation;
        let cancel = CancellationToken::new();
        self.canvas.render_cancel = Some(cancel.clone());
        let task_cancel = cancel.clone();

        let task = self
            .tasks
            .spawn_blocking_named("Restore complete presentation", move || {
                encode_display_payload(&complete_rgba, &cancel)
            });

        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.canvas.render_generation != render_gen || task_cancel.is_cancelled() {
                    return;
                }
                this.canvas.render_cancel = None;
                if let Ok(Ok(payload)) = result
                    && this.can_install_complete_presentation(edit_rev)
                    && let Some(doc) = &mut this.session.document
                {
                    doc.cache
                        .install_display_output(payload, edit_rev, render_gen, cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(in crate::pages::workspace::ui) fn rebuild_textures(&mut self, cx: &mut Context<Self>) {
        self.cancel_gesture_render();
        let Some(doc) = &self.session.document else {
            return;
        };
        let base_rgba = Arc::clone(&doc.decoded.rgba);
        let cached_base = doc.cache.transformed_base.clone();
        let transform = doc.transform;
        let annotations = doc.annotations.clone();
        let edit_rev = doc.edit_revision;

        if let Some(cancel) = self.canvas.render_cancel.take() {
            cancel.cancel();
        }
        self.cancel_svg_rerender();
        self.canvas.svg_rerender_scale = None;
        self.canvas.render_generation = self.canvas.render_generation.wrapping_add(1);
        let render_gen = self.canvas.render_generation;
        let cancel = CancellationToken::new();
        self.canvas.render_cancel = Some(cancel.clone());

        let task = self
            .tasks
            .spawn_blocking_named("Composite image textures", move || {
                build_render_textures(base_rgba, cached_base, transform, &annotations, &cancel)
            });

        let cancel_check = self.canvas.render_cancel.clone();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.canvas.render_generation != render_gen {
                    return;
                }
                if cancel_check
                    .as_ref()
                    .is_some_and(CancellationToken::is_cancelled)
                {
                    return;
                }
                this.canvas.render_cancel = None;
                match result {
                    Ok(Ok(prepared)) => {
                        let install_presented = this.can_install_complete_presentation(edit_rev);
                        if let Some(doc) = &mut this.session.document
                            && doc.edit_revision == edit_rev
                        {
                            if install_presented {
                                install_prepared_cache(
                                    &mut doc.cache,
                                    prepared,
                                    transform,
                                    render_gen,
                                    edit_rev,
                                    cx,
                                );
                                this.schedule_svg_rerender_if_needed(cx);
                            } else {
                                doc.cache.store_complete_only(prepared, transform, edit_rev);
                            }
                            cx.notify();
                        }
                    }
                    Ok(Err(ImageError::Cancelled)) => {}
                    Ok(Err(err)) => {
                        show_report::<LoadNotice>(
                            Level::Error,
                            t!(@cx, "toast-render-failed", error = err.to_string()),
                            cx,
                        );
                    }
                    Err(err) => {
                        show_report::<LoadNotice>(
                            Level::Error,
                            t!(@cx, "toast-render-failed", error = err.to_string()),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    pub(in crate::pages::workspace::ui) fn cancel_svg_rerender(&mut self) {
        self.canvas.svg_rerender_generation = self.canvas.svg_rerender_generation.wrapping_add(1);
        if let Some(cancel) = self.canvas.svg_rerender_cancel.take() {
            cancel.cancel();
        }
    }

    pub(in crate::pages::workspace::ui) fn schedule_svg_rerender_if_needed(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.session.document else {
            return;
        };
        if doc.format() != Format::Svg || doc.has_unbaked_ops() {
            return;
        }
        let edit_rev = doc.edit_revision;
        let scale = self.canvas.transform.scale;
        if scale <= 1.0 {
            if self.canvas.svg_rerender_scale.is_some() {
                let base_rgba = Arc::clone(&doc.decoded.rgba);
                self.cancel_svg_rerender();
                self.canvas.svg_rerender_scale = None;
                let svg_gen = self.canvas.svg_rerender_generation;
                let cancel = CancellationToken::new();
                self.canvas.svg_rerender_cancel = Some(cancel.clone());
                let task_cancel = cancel.clone();
                let render_gen = self.canvas.render_generation;
                let task = self
                    .tasks
                    .spawn_blocking_named("Restore SVG base texture", move || {
                        build_svg_zoom_textures(base_rgba, &cancel)
                    });
                cx.spawn(async move |this, cx| {
                    let result = task.await;
                    let _ = this.update(cx, |this, cx| {
                        if this.canvas.render_generation != render_gen
                            || this.canvas.svg_rerender_generation != svg_gen
                            || task_cancel.is_cancelled()
                        {
                            return;
                        }
                        this.canvas.svg_rerender_cancel = None;
                        if let Ok(Ok(payload)) = result
                            && this.can_install_complete_presentation(edit_rev)
                            && let Some(doc) = &mut this.session.document
                            && !doc.has_unbaked_ops()
                        {
                            install_svg_payload(&mut doc.cache, payload, edit_rev, render_gen, cx);
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            return;
        }

        if scale <= 1.05 {
            return;
        }
        if self
            .canvas
            .svg_rerender_scale
            .is_some_and(|prev| (prev - scale).abs() / prev.max(0.01) < 0.12)
        {
            return;
        }

        let source_bytes = Arc::clone(&doc.decoded.source_bytes);
        let (base_w, base_h) = doc.base_dimensions();
        self.cancel_svg_rerender();
        let svg_gen = self.canvas.svg_rerender_generation;
        let cancel = CancellationToken::new();
        self.canvas.svg_rerender_cancel = Some(cancel.clone());
        let task_cancel = cancel.clone();
        let render_gen = self.canvas.render_generation;
        let target_w = ((base_w as f32) * scale).round().clamp(1.0, 16_384.0) as u32;
        let target_h = ((base_h as f32) * scale).round().clamp(1.0, 16_384.0) as u32;

        let task = self
            .tasks
            .spawn_blocking_named("Render scaled SVG", move || {
                if cancel.is_cancelled() {
                    return Err(ImageError::Cancelled);
                }
                let rgba = render_svg_scaled(&source_bytes, target_w, target_h)?;
                build_svg_zoom_textures(Arc::new(rgba), &cancel)
            });

        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.canvas.render_generation != render_gen
                    || this.canvas.svg_rerender_generation != svg_gen
                    || task_cancel.is_cancelled()
                {
                    return;
                }
                this.canvas.svg_rerender_cancel = None;
                match result {
                    Ok(Ok(payload)) => {
                        if this.can_install_complete_presentation(edit_rev)
                            && let Some(doc) = &mut this.session.document
                            && doc.format() == Format::Svg
                            && !doc.has_unbaked_ops()
                        {
                            this.canvas.svg_rerender_scale = Some(scale);
                            install_svg_payload(&mut doc.cache, payload, edit_rev, render_gen, cx);
                            cx.notify();
                        }
                    }
                    Ok(Err(ImageError::Cancelled)) => {}
                    Ok(Err(err)) => {
                        tracing::debug!(error = %err, "scaled SVG zoom render skipped");
                    }
                    Err(err) => {
                        tracing::debug!(error = %err, "scaled SVG zoom worker aborted");
                    }
                }
            });
        })
        .detach();
    }
}

fn install_prepared_cache(
    cache: &mut RenderCache,
    prepared: PreparedTextures,
    transform: TransformState,
    render_gen: u64,
    edit_rev: u64,
    cx: &mut gpui_kit::App,
) {
    cache.install_complete(prepared, transform, render_gen, edit_rev, cx);
}

fn install_svg_payload(
    cache: &mut RenderCache,
    payload: PreparedPayload,
    revision: u64,
    render_gen: u64,
    cx: &mut gpui_kit::App,
) {
    cache.install_display_output(payload, revision, render_gen, cx);
}
