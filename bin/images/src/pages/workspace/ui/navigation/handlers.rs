//! Inline filename rename handling with `load_generation` and concurrent-origin guards.

use std::path::PathBuf;

use gds::{
    i18n::t,
    service::image::source::{FileStamp, Origin},
    ui::notifications::Level,
};
use gpui_kit::{Context, Window};

use super::*;
use crate::{entities::catalog::CatalogEntryMeta, shared::i18n::text};

enum RenameOutcome {
    Done {
        old_path: PathBuf,
        new_path: PathBuf,
        byte_len: u64,
        modified: Option<std::time::SystemTime>,
        stamp: Option<FileStamp>,
    },
    AlreadyExists(String),
    Failed(String),
}

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn begin_path_edit(
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
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.session.editing_path = true;
        self.session.suppress_blur_commit = false;
        self.focus.path_input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn cancel_path_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session.editing_path = false;
        self.session.suppress_blur_commit = true;
        self.focus.canvas.focus(window, cx);
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn commit_path_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.session.editing_path {
            return;
        }
        let raw_name = self.focus.path_input.read(cx).value().trim().to_owned();
        self.session.editing_path = false;
        self.focus.canvas.focus(window, cx);

        let invalid = raw_name.is_empty()
            || raw_name == "."
            || raw_name == ".."
            || raw_name.contains(['/', '\\', '\0']);
        if invalid {
            show_report::<RenameNotice>(Level::Warning, text("rename-error-invalid", cx), cx);
            cx.notify();
            return;
        }

        let Some(doc) = &self.session.document else {
            cx.notify();
            return;
        };
        let Origin::LocalFile { path: old_path, .. } = &doc.decoded.origin else {
            cx.notify();
            return;
        };
        let old_name = old_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if raw_name == old_name {
            cx.notify();
            return;
        }
        if self.is_origin_busy(&doc.decoded.origin) {
            cx.notify();
            return;
        }

        let Some(parent) = old_path.parent() else {
            show_report::<RenameNotice>(Level::Warning, text("rename-error-invalid", cx), cx);
            cx.notify();
            return;
        };
        let new_path = parent.join(&raw_name);
        let old_path = old_path.clone();
        let old_origin = doc.decoded.origin.clone();
        let doc_gen = self.session.load_generation;
        self.runtime.in_flight_origin_op = Some(old_origin.clone());
        cx.notify();

        let task = self.tasks.spawn_blocking_named("Rename image", move || {
            if new_path.exists() {
                return RenameOutcome::AlreadyExists(raw_name);
            }
            if let Err(err) = std::fs::rename(&old_path, &new_path) {
                return RenameOutcome::Failed(err.to_string());
            }
            let (byte_len, modified) = read_file_meta(&new_path);
            let stamp = FileStamp::capture(&new_path).ok();
            RenameOutcome::Done {
                old_path,
                new_path,
                byte_len,
                modified,
                stamp,
            }
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.runtime.in_flight_origin_op.as_ref() == Some(&old_origin) {
                    this.runtime.in_flight_origin_op = None;
                }
                match result {
                    Ok(RenameOutcome::Done {
                        old_path,
                        new_path,
                        byte_len,
                        modified,
                        stamp,
                    }) => {
                        let new_meta = CatalogEntryMeta::new(&new_path, Some(byte_len), modified);
                        this.session
                            .catalog
                            .rename_path(&old_path, new_path.clone(), new_meta);
                        if this.session.load_generation == doc_gen
                            && let Some(doc) = &mut this.session.document
                        {
                            doc.decoded.origin = Origin::LocalFile {
                                path: new_path,
                                stamp,
                            };
                            this.sync_window_title(window, cx);
                        }
                        show_report::<RenameNotice>(Level::Success, text("rename-done", cx), cx);
                    }
                    Ok(RenameOutcome::AlreadyExists(name)) => {
                        show_report::<RenameNotice>(
                            Level::Warning,
                            t!(@cx, "toast-rename-exists", name = name),
                            cx,
                        );
                    }
                    Ok(RenameOutcome::Failed(error)) => {
                        show_report::<RenameNotice>(
                            Level::Error,
                            t!(@cx, "toast-rename-failed", error = error),
                            cx,
                        );
                    }
                    Err(err) => {
                        show_report::<RenameNotice>(
                            Level::Error,
                            t!(@cx, "toast-rename-failed", error = err.to_string()),
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
