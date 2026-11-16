//! Image decoding, preview fallback, texture uploads, SVG zoom rerender, and discard guard.

use std::{cell::Cell, path::PathBuf, rc::Rc, sync::Arc};

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
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{
    entities::{
        document::{
            DocumentState, PreparedPayload, PreparedTextures, RenderCache, build_render_textures,
            build_svg_zoom_textures,
        },
        metadata::{self, MetadataState},
    },
    features::navigation::open_file_dialog,
    shared::{
        clipboard::ClipboardExt as _,
        geometry::{FitMode, Size2, ViewportTransform},
        i18n::text,
    },
};

const PREVIEW_FALLBACK_MAX_EDGE: u32 = 2048;

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
        self.canvas.tool_fsm.cancel_active();
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

    pub(in crate::pages::workspace::ui) fn rebuild_textures(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = &self.session.document else {
            return;
        };
        let base_rgba = Arc::clone(&doc.decoded.rgba);
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
                build_render_textures(base_rgba, transform, &annotations, &cancel)
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
                        if let Some(doc) = &mut this.session.document {
                            install_prepared_cache(
                                &mut doc.cache,
                                prepared,
                                render_gen,
                                edit_rev,
                                cx,
                            );
                        }
                        this.schedule_svg_rerender_if_needed(cx);
                        cx.notify();
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
                            && let Some(doc) = &mut this.session.document
                            && !doc.has_unbaked_ops()
                        {
                            install_svg_payload(&mut doc.cache, payload, render_gen, cx);
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
                        if let Some(doc) = &mut this.session.document
                            && doc.format() == Format::Svg
                            && !doc.has_unbaked_ops()
                        {
                            this.canvas.svg_rerender_scale = Some(scale);
                            install_svg_payload(&mut doc.cache, payload, render_gen, cx);
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
    render_gen: u64,
    edit_rev: u64,
    cx: &mut gpui_kit::App,
) {
    cache.install(prepared, render_gen, edit_rev, cx);
}

fn install_svg_payload(
    cache: &mut RenderCache,
    payload: PreparedPayload,
    render_gen: u64,
    cx: &mut gpui_kit::App,
) {
    cache.install_display_output(payload, render_gen, cx);
}
