//! File save, Save As, metadata & tag commits, privacy stripping, and desktop file actions.

use std::{path::PathBuf, sync::Arc};

use gds::{
    i18n::t,
    service::image::{
        Error as ImageError,
        io::SaveMode,
        metadata::{MetadataBundle, inspect_bytes},
        source::Origin,
    },
    ui::notifications::Level,
};
use gpui_kit::{Context, Window};

use super::*;
use crate::{
    entities::{metadata::MetadataState, preferences::PreferencesExt as _},
    features::{
        desktop_actions::{move_to_trash, open_with_chooser, reveal_in_files},
        export::{
            build_save_plan, execute_save_blocking, prompt_save_as_path, suggested_save_filename,
        },
        metadata_edit::{
            can_losslessly_commit_metadata, clear_privacy_in_draft,
            commit_metadata_and_tags_blocking,
        },
    },
    shared::i18n::text,
};

struct SaveTaskResult {
    outcome: crate::features::export::SaveOutcome,
    bundle: MetadataBundle,
    byte_len: u64,
    modified: Option<std::time::SystemTime>,
}

struct MetadataTaskResult {
    outcome: crate::features::metadata_edit::MetadataCommitOutcome,
    byte_len: u64,
    modified: Option<std::time::SystemTime>,
}

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn clear_privacy_metadata(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.session.document else {
            return;
        };
        if self.is_origin_busy(&doc.decoded.origin) {
            return;
        }
        let can_commit = can_losslessly_commit_metadata(doc);
        let Some(meta) = &mut self.session.metadata else {
            return;
        };
        if can_commit {
            clear_privacy_in_draft(meta);
            self.run_metadata_commit(true, window, cx);
        } else {
            clear_privacy_in_draft(meta);
            self.sync_window_title(window, cx);
            show_report::<MetadataNotice>(
                Level::Info,
                text("toast-metadata-draft-cleared", cx),
                cx,
            );
            cx.notify();
        }
    }

    pub(in crate::pages::workspace::ui) fn run_metadata_commit(
        &mut self,
        strip_privacy: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.session.document else {
            return;
        };
        let Some(meta) = &self.session.metadata else {
            return;
        };
        let Origin::LocalFile { path, .. } = &doc.decoded.origin else {
            return;
        };
        if self.is_origin_busy(&doc.decoded.origin) {
            return;
        }
        let path = path.clone();
        let origin = doc.decoded.origin.clone();
        let doc_gen = self.session.load_generation;
        let req = build_metadata_request(path.clone(), doc, meta, strip_privacy);
        self.runtime.is_saving = true;
        self.runtime.in_flight_origin_op = Some(origin.clone());
        cx.notify();

        let task = self
            .tasks
            .spawn_blocking_named("Commit image metadata", move || {
                let outcome = commit_metadata_and_tags_blocking(req)?;
                let (byte_len, modified) = read_file_meta(&path);
                Ok::<_, ImageError>(MetadataTaskResult {
                    outcome,
                    byte_len,
                    modified,
                })
            });

        let saved_path = match &origin {
            Origin::LocalFile { path, .. } => path.clone(),
            Origin::Memory { .. } => unreachable!(),
        };

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.runtime.is_saving = false;
                if this.runtime.in_flight_origin_op.as_ref() == Some(&origin) {
                    this.runtime.in_flight_origin_op = None;
                }
                match result {
                    Ok(Ok(done)) => {
                        this.session.catalog.update_entry_meta(
                            &saved_path,
                            done.byte_len,
                            done.modified,
                        );
                        if this.session.load_generation == doc_gen {
                            if let Some(doc) = &mut this.session.document {
                                if let Some(stamp) = done.outcome.updated_stamp
                                    && let Origin::LocalFile { stamp: slot, .. } =
                                        &mut doc.decoded.origin
                                {
                                    *slot = Some(stamp);
                                }
                                if let Some(bytes) = done.outcome.rewritten_bytes {
                                    let fmt = doc.format();
                                    doc.decoded.source_bytes = Arc::clone(&bytes);
                                    if let Ok(bundle) =
                                        inspect_bytes(&bytes, fmt, Some(done.byte_len))
                                    {
                                        doc.decoded.metadata = bundle.clone();
                                        if let Some(meta) = &mut this.session.metadata {
                                            *meta = MetadataState::from_bundle(
                                                bundle,
                                                done.outcome.updated_tags.clone(),
                                            );
                                        }
                                    } else if let Some(meta) = &mut this.session.metadata {
                                        meta.original_tags = done.outcome.updated_tags.clone();
                                        meta.tags = done.outcome.updated_tags;
                                        meta.metadata_dirty = false;
                                        meta.tags_dirty = false;
                                    }
                                } else if let Some(meta) = &mut this.session.metadata {
                                    meta.original_tags = done.outcome.updated_tags.clone();
                                    meta.tags = done.outcome.updated_tags;
                                    meta.tags_dirty = false;
                                }
                            }
                            this.sync_window_title(window, cx);
                        }
                        let msg_key = if strip_privacy {
                            "toast-metadata-cleared"
                        } else {
                            "toast-metadata-updated"
                        };
                        show_report::<MetadataNotice>(Level::Success, text(msg_key, cx), cx);
                    }
                    Ok(Err(ImageError::ExternalConflict(conflict_path))) => {
                        show_report::<MetadataNotice>(
                            Level::Error,
                            t!(
                                @cx,
                                "toast-save-conflict",
                                path = conflict_path.display().to_string()
                            ),
                            cx,
                        );
                    }
                    Ok(Err(err)) => {
                        show_report::<MetadataNotice>(
                            Level::Error,
                            t!(@cx, "toast-metadata-failed", error = err.to_string()),
                            cx,
                        );
                    }
                    Err(err) => {
                        show_report::<MetadataNotice>(
                            Level::Error,
                            t!(@cx, "toast-metadata-failed", error = err.to_string()),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::pages::workspace::ui) fn save_current_image(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.session.document else {
            return;
        };
        if self.is_origin_busy(&doc.decoded.origin) {
            return;
        }
        match &doc.decoded.origin {
            Origin::LocalFile { path, stamp } if doc.format().capabilities().can_encode => {
                self.save_to_path(
                    path.clone(),
                    SaveMode::Overwrite {
                        expected: stamp.clone(),
                    },
                    window,
                    cx,
                );
            }
            Origin::Memory { .. } if self.runtime.launch.workflow.output.is_some() => {
                if let Some(target) = self.runtime.launch.workflow.output.clone() {
                    self.save_to_path(target, SaveMode::SaveAs, window, cx);
                }
            }
            _ => self.save_image_as(window, cx),
        }
    }

    pub(in crate::pages::workspace::ui) fn save_image_as(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.session.document else {
            return;
        };
        if self.is_origin_busy(&doc.decoded.origin) {
            return;
        }
        let suggested = suggested_save_filename(doc);
        let dir = match &doc.decoded.origin {
            Origin::LocalFile { path, .. } => path.parent().map(std::path::Path::to_path_buf),
            Origin::Memory { .. } => self
                .runtime
                .launch
                .workflow
                .output
                .as_deref()
                .and_then(std::path::Path::parent)
                .map(std::path::Path::to_path_buf),
        };
        let prompt = prompt_save_as_path(&suggested, dir.as_deref(), cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(path) = prompt.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.save_to_path(path, SaveMode::SaveAs, window, cx);
            });
        })
        .detach();
    }

    pub(in crate::pages::workspace::ui) fn save_to_path(
        &mut self,
        target_path: PathBuf,
        save_mode: SaveMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.session.document else {
            return;
        };
        let Some(meta) = &self.session.metadata else {
            return;
        };
        if self.is_origin_busy(&doc.decoded.origin) {
            return;
        }
        let prefs = cx.gds().preferences().current();
        let plan = build_save_plan(target_path.clone(), save_mode, doc, meta, &prefs);
        let doc_gen = self.session.load_generation;
        let origin = doc.decoded.origin.clone();
        self.runtime.is_saving = true;
        self.runtime.in_flight_origin_op = Some(origin.clone());
        cx.notify();

        let saved_path = target_path.clone();
        let task = self.tasks.spawn_blocking_named("Save image", move || {
            let outcome = execute_save_blocking(plan)?;
            let (byte_len, modified) = read_file_meta(&saved_path);
            let bundle = inspect_bytes(
                &outcome.source_bytes,
                outcome.format,
                Some(outcome.source_bytes.len() as u64),
            )?;
            Ok::<_, ImageError>(SaveTaskResult {
                outcome,
                bundle,
                byte_len,
                modified,
            })
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.runtime.is_saving = false;
                if this.runtime.in_flight_origin_op.as_ref() == Some(&origin) {
                    this.runtime.in_flight_origin_op = None;
                }
                match result {
                    Ok(Ok(done)) => {
                        this.finish_save_success(&target_path, doc_gen, done, window, cx);
                    }
                    Ok(Err(ImageError::ExternalConflict(conflict_path))) => {
                        show_report::<SaveNotice>(
                            Level::Error,
                            t!(
                                @cx,
                                "toast-save-conflict",
                                path = conflict_path.display().to_string()
                            ),
                            cx,
                        );
                    }
                    Ok(Err(err)) => {
                        show_report::<SaveNotice>(
                            Level::Error,
                            t!(@cx, "toast-save-failed", error = err.to_string()),
                            cx,
                        );
                    }
                    Err(err) => {
                        show_report::<SaveNotice>(
                            Level::Error,
                            t!(@cx, "toast-save-failed", error = err.to_string()),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn finish_save_success(
        &mut self,
        target_path: &std::path::Path,
        doc_gen: u64,
        done: SaveTaskResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        this_update_catalog_after_save(
            &mut self.session.catalog,
            target_path,
            done.byte_len,
            done.modified,
        );

        let mut trigger_copy_on_save = false;
        let mut trigger_early_exit = false;
        if self.session.load_generation == doc_gen {
            let path_buf = target_path.to_path_buf();
            let mut needs_texture_rebuild = false;
            let mut should_clear_text = false;
            if let Some(doc) = &mut self.session.document {
                if let Some(composited) = done.outcome.composited_rgba {
                    doc.apply_saved_pixels(composited);
                    self.canvas.history.clear();
                    self.canvas.tool_fsm.cancel_active();
                    should_clear_text = true;
                    needs_texture_rebuild = true;
                } else {
                    doc.mark_clean();
                }
                doc.decoded.origin = Origin::LocalFile {
                    path: path_buf.clone(),
                    stamp: Some(done.outcome.stamp),
                };
                doc.decoded.format = done.outcome.format;
                doc.decoded.source_bytes = done.outcome.source_bytes;
                doc.decoded.metadata = done.bundle.clone();
            }
            if should_clear_text {
                self.clear_text_overlay_input(window, cx);
            }
            if let Some(meta) = &mut self.session.metadata {
                *meta = MetadataState::from_bundle(done.bundle, done.outcome.saved_tags);
            }
            self.sync_window_title(window, cx);
            if needs_texture_rebuild {
                self.rebuild_textures(cx);
            }
            if self
                .session
                .catalog
                .current_path()
                .map(std::path::PathBuf::as_path)
                != Some(target_path)
            {
                self.start_sibling_scan(path_buf, cx);
            }
            trigger_copy_on_save = self.runtime.launch.workflow.copy_on_save;
            trigger_early_exit = !trigger_copy_on_save && self.runtime.launch.workflow.early_exit;
        }

        show_report::<SaveNotice>(
            Level::Success,
            t!(
                @cx,
                "toast-save-success",
                path = target_path.display().to_string()
            ),
            cx,
        );

        if trigger_copy_on_save {
            self.copy_to_clipboard(window, cx);
        } else if trigger_early_exit {
            window.remove_window();
        }
    }

    pub(in crate::pages::workspace::ui) fn reveal_active_file(&self, cx: &mut Context<Self>) {
        if let Some(doc) = &self.session.document
            && let Origin::LocalFile { path, .. } = &doc.decoded.origin
        {
            reveal_in_files(path.clone(), &self.tasks, cx);
        }
    }

    pub(in crate::pages::workspace::ui) fn open_active_file_with(&self, cx: &mut Context<Self>) {
        if let Some(doc) = &self.session.document
            && let Origin::LocalFile { path, .. } = &doc.decoded.origin
        {
            open_with_chooser(path.clone(), &self.tasks, cx);
        }
    }

    pub(in crate::pages::workspace::ui) fn move_active_to_trash(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.session.document else {
            return;
        };
        let Origin::LocalFile { path, .. } = &doc.decoded.origin else {
            return;
        };
        if self.is_origin_busy(&doc.decoded.origin) {
            return;
        }
        let path = path.clone();
        let origin = doc.decoded.origin.clone();
        let doc_gen = self.session.load_generation;
        self.runtime.in_flight_origin_op = Some(origin.clone());
        let task = move_to_trash(path.clone(), &self.tasks, cx);
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.runtime.in_flight_origin_op.as_ref() == Some(&origin) {
                    this.runtime.in_flight_origin_op = None;
                }
                match result {
                    Ok(()) => {
                        let next_path = this.session.catalog.remove_path(&path);
                        show_report::<TrashNotice>(
                            Level::Success,
                            t!(
                                @cx,
                                "toast-trash-success",
                                path = path.display().to_string()
                            ),
                            cx,
                        );
                        if this.session.load_generation == doc_gen {
                            if let Some(next) = next_path {
                                this.open_local_path(next, window, cx);
                            } else {
                                if let Some(mut doc) = this.session.document.take() {
                                    doc.cache.release_textures(cx);
                                }
                                this.session.metadata = None;
                                this.session.editing_path = false;
                                this.canvas.history.clear();
                                this.canvas.tool_fsm.cancel_active();
                                this.clear_text_overlay_input(window, cx);
                                this.sync_window_title(window, cx);
                                this.refresh_clipboard_offer(cx);
                            }
                        }
                    }
                    Err(err) => {
                        show_report::<TrashNotice>(
                            Level::Error,
                            t!(@cx, "toast-trash-failed", error = err.to_string()),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn this_update_catalog_after_save(
    catalog: &mut crate::entities::catalog::CatalogState,
    path: &std::path::Path,
    byte_len: u64,
    modified: Option<std::time::SystemTime>,
) {
    catalog.update_entry_meta(path, byte_len, modified);
}
