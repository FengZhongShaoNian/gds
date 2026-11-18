//! File, clipboard, trash, history, volume, and sort workflows.

use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use gds::i18n as gds_i18n;
use gds::service::desktop::{Desktop, OpenMode, OpenOutcome};
use tokio_util::sync::CancellationToken;
use zgui::reactive::spawn_local;

use super::{
    model::{Contents, Operation, Row},
    name_form::{NameForm, NamePurpose},
    ui::Browser,
};
use crate::{
    entities::{
        file::columns::Column,
        location::{
            Location, scheme,
            volumes::{self, DeviceAction},
        },
    },
    features::{
        file_operations::{
            Batch, Kind as DeleteKind, history, names, new_folder,
            paste::{self, Inspection, PassOutcome, PasteJob},
            recycle, rename, transfer,
        },
        pick_files,
    },
    shared::{
        i18n,
        lib::clipboard::{self, Clipboard, Payload},
    },
};

impl Browser {
    pub(super) fn refresh_affected(&self, paths: &[PathBuf]) {
        self.refresh_after_file_operation(paths, false);
    }

    fn refresh_after_file_operation(&self, paths: &[PathBuf], trash_affected: bool) {
        let mut dir_tabs = Vec::new();
        let mut trash_tabs = Vec::new();
        let mut has_tag_tabs = false;
        let parents: HashSet<PathBuf> = paths
            .iter()
            .filter_map(|path| path.parent().map(Path::to_path_buf))
            .collect();
        self.read_untracked(|state| {
            for tab in &state.tabs {
                match tab.contents {
                    Contents::Directory => {
                        if let Some(dir) = tab.navigation.path()
                            && (paths.iter().any(|path| path == dir) || parents.contains(dir))
                        {
                            dir_tabs.push(tab.id);
                        }
                    }
                    Contents::Trash(_) if trash_affected => {
                        trash_tabs.push(tab.id);
                    }
                    Contents::Tags { .. } => {
                        has_tag_tabs = true;
                    }
                    _ => {}
                }
            }
        });
        for id in dir_tabs {
            self.refresh_tab(id);
        }
        for id in trash_tabs {
            self.refresh_tab(id);
        }
        if has_tag_tabs {
            self.refresh_tag_tabs();
        }
    }

    pub(super) fn begin_operation(&self, label: &str) -> Option<CancellationToken> {
        let cancel = CancellationToken::new();
        let started = self.update(|state| {
            if state.operation.busy() {
                return false;
            }
            state.operation = Operation::Running {
                label: label.to_owned(),
                cancel: cancel.clone(),
            };
            true
        });
        if started {
            self.changed();
            Some(cancel)
        } else {
            None
        }
    }

    pub(super) fn end_operation(&self) {
        self.update(|state| state.operation = Operation::Idle);
        self.changed();
    }

    pub(super) fn open_selected(&self) {
        let in_picker = self.read_untracked(|state| state.picker.is_some());
        let selected = self.read_untracked(|state| {
            let tab = state.tab();
            let view = tab.navigation.view();
            tab.rows
                .iter()
                .enumerate()
                .filter_map(|(idx, row)| {
                    view.selection
                        .contains(&row.key())
                        .then_some((idx, row.clone()))
                })
                .collect::<Vec<_>>()
        });
        if selected.is_empty() {
            if in_picker {
                self.accept_picker();
            }
            return;
        }
        if selected.len() == 1 {
            let (index, row) = &selected[0];
            if in_picker
                && let Row::File(entry) = row
                && !pick_files::Projection::is_directory(entry)
            {
                self.accept_picker();
                return;
            }
            self.open_row(*index, false);
            return;
        }
        if in_picker {
            self.accept_picker();
            return;
        }
        for (idx, row) in selected {
            match row {
                Row::File(entry) if pick_files::Projection::is_directory(&entry) => {
                    let location = self.local_location(&entry.path);
                    self.new_tab(Some(location));
                }
                Row::File(entry) => {
                    self.open_file(entry.path.clone(), OpenMode::DefaultApplication)
                }
                _ => self.open_row(idx, false),
            }
        }
    }

    pub(super) fn open_row(&self, index: usize, new_tab: bool) {
        let (row, in_picker) = self
            .read_untracked(|state| (state.tab().rows.get(index).cloned(), state.picker.is_some()));
        let Some(row) = row else {
            return;
        };
        match row {
            Row::File(entry) => {
                if pick_files::Projection::is_directory(&entry) {
                    let location = self.local_location(&entry.path);
                    if new_tab && !in_picker {
                        self.new_tab(Some(location));
                    } else {
                        self.open(location);
                    }
                } else if in_picker {
                    self.activate_picker_path(entry.path.clone());
                } else {
                    self.open_file(entry.path.clone(), OpenMode::DefaultApplication);
                }
            }
            Row::Place(location) => {
                if new_tab && !in_picker {
                    self.new_tab(Some(location));
                } else {
                    self.open(location);
                }
            }
            Row::Tag(tag) => {
                if let Ok(address) = scheme::TagAddress::new(&tag) {
                    let location = Location::Tags(address);
                    if new_tab && !in_picker {
                        self.new_tab(Some(location));
                    } else {
                        self.open(location);
                    }
                }
            }
            Row::Remote(entry) => self.activate_remote(entry, new_tab),
            Row::Volume(volume) => self.open_volume(&volume.id, new_tab),
            Row::Trash(_) => {}
        }
    }

    pub(super) fn open_file(&self, path: PathBuf, mode: OpenMode) {
        self.work(
            "Open file",
            async move {
                Desktop::default()
                    .open_file(&path, mode)
                    .await
                    .map_err(|error| error.to_string())
            },
            |browser, result| match result {
                Ok(OpenOutcome::Opened | OpenOutcome::Cancelled) => {}
                Err(error) => browser.report_notice(error),
            },
        );
    }

    pub(super) fn new_folder(&self) {
        let info = self.read_untracked(|state| {
            if state.operation.busy() {
                return None;
            }
            let tab = state.tab();
            if !matches!(tab.contents, Contents::Directory) {
                return None;
            }
            let directory = tab.navigation.path()?.to_path_buf();
            let folder_label = self.location_title(tab.navigation.location());
            let reserved: HashSet<OsString> = tab
                .navigation
                .directory()
                .map(|snap| {
                    snap.entries
                        .iter()
                        .map(|entry| entry.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            Some((directory, folder_label, reserved))
        });
        let Some((directory, folder_label, reserved)) = info else {
            return;
        };
        let default_name = i18n::text_untracked("new-folder-default-name");
        let suggestion = names::free_name(&directory, OsStr::new(&default_name), &reserved)
            .unwrap_or_else(|_| OsString::from(&default_name));
        let title = gds::t_untracked!("new-folder-title", folder = folder_label);
        let form = NameForm::new(NamePurpose::NewFolder, title, suggestion);
        let browser = self.clone();
        self.name_form(form, move |name| {
            let Some(cancel) = browser.begin_operation(&i18n::text_untracked("new-folder-working"))
            else {
                return;
            };
            let parent = directory.clone();
            let request = new_folder::Request {
                directory: parent.clone(),
                name,
            };
            let task = request.spawn(&browser.scope(), cancel);
            let weak = browser.downgrade();
            browser.0.owner.with(|| {
                drop(spawn_local(async move {
                    let result = task.await;
                    let Some(browser) = weak.upgrade() else {
                        return;
                    };
                    browser.end_operation();
                    match result {
                        Ok(Ok(created)) => {
                            let label = created
                                .path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            if let Some(expected) = created.snapshot {
                                browser.update(|state| {
                                    state.history.record(history::Entry::new(
                                        history::Kind::NewFolder,
                                        vec![history::Step::trash_empty_directory(
                                            created.path.clone(),
                                            expected,
                                        )],
                                    ));
                                });
                            }
                            browser.reveal_paths(vec![created.path.clone()]);
                            browser.refresh_affected(&[parent]);
                            browser
                                .report_success(gds::t_untracked!("new-folder-done", name = label));
                            browser.focus_content();
                        }
                        Ok(Err(error)) => {
                            let message = match error {
                                new_folder::Error::Exists { name, .. } => {
                                    gds::t_untracked!("new-folder-exists", name = name)
                                }
                                other => {
                                    gds::t_untracked!(
                                        "new-folder-failed",
                                        error = other.to_string()
                                    )
                                }
                            };
                            browser.report_notice(message);
                        }
                        Err(error) => {
                            browser.report_notice(gds::t_untracked!(
                                "new-folder-failed",
                                error = error.to_string()
                            ));
                        }
                    }
                }));
            });
        });
    }

    pub(super) fn rename(&self) {
        let target = self.read_untracked(|state| {
            if state.operation.busy() {
                return None;
            }
            let selected = state.tab().selected();
            if selected.len() != 1 {
                return None;
            }
            selected.into_iter().next()
        });
        let Some(row) = target else {
            return;
        };
        match row {
            Row::Remote(entry) => self.rename_remote(entry),
            Row::Volume(volume) => self.rename_volume(volume),
            Row::File(entry) => {
                let source = entry.path.clone();
                let current_name = entry.name.clone();
                let display = current_name.to_string_lossy().into_owned();
                let form = NameForm::new(
                    NamePurpose::Rename,
                    gds::t_untracked!("rename-title", name = display),
                    current_name.clone(),
                );
                let browser = self.clone();
                self.name_form(form, move |name| {
                    if name == current_name {
                        browser.focus_content();
                        return;
                    }
                    let Some(cancel) =
                        browser.begin_operation(&i18n::text_untracked("rename-working"))
                    else {
                        return;
                    };
                    let src = source.clone();
                    let request = rename::Request {
                        source: src.clone(),
                        name,
                    };
                    let task = request.spawn(&browser.scope(), cancel);
                    let weak = browser.downgrade();
                    browser.0.owner.with(|| {
                        drop(spawn_local(async move {
                            let result = task.await;
                            let Some(browser) = weak.upgrade() else {
                                return;
                            };
                            browser.end_operation();
                            match result {
                                Ok(Ok(renamed)) => {
                                    let display_new = renamed
                                        .destination
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_default();
                                    if let Some(expected) = renamed.snapshot {
                                        browser.update(|state| {
                                            state.history.record(history::Entry::new(
                                                history::Kind::Rename,
                                                vec![history::Step::Move {
                                                    source: renamed.destination.clone(),
                                                    destination: src.clone(),
                                                    expected,
                                                }],
                                            ));
                                        });
                                    }
                                    browser.reveal_paths(vec![renamed.destination.clone()]);
                                    browser.refresh_affected(&[src, renamed.destination]);
                                    browser.report_success(gds::t_untracked!(
                                        "rename-done",
                                        name = display_new
                                    ));
                                    browser.focus_content();
                                }
                                Ok(Err(error)) => {
                                    let message = match error {
                                        rename::Error::Exists { name, .. } => {
                                            gds::t_untracked!("rename-target-exists", name = name)
                                        }
                                        rename::Error::Name(err) => {
                                            i18n::text_untracked(err.label_id())
                                        }
                                        other => gds::t_untracked!(
                                            "rename-failed",
                                            error = other.to_string()
                                        ),
                                    };
                                    browser.report_notice(message);
                                }
                                Err(error) => {
                                    browser.report_notice(gds::t_untracked!(
                                        "rename-failed",
                                        error = error.to_string()
                                    ));
                                }
                            }
                        }));
                    });
                });
            }
            Row::Place(_) | Row::Trash(_) | Row::Tag(_) => {}
        }
    }

    pub(super) fn delete(&self, permanent: bool) {
        enum DeleteTarget {
            None,
            Remote(Vec<Row>),
            Trash(bool),
            Tags(Vec<Row>),
            Local(Vec<PathBuf>),
        }
        let target = self.read_untracked(|state| {
            if state.operation.busy() {
                return DeleteTarget::None;
            }
            let tab = state.tab();
            let selected = tab.selected();
            if selected.is_empty() {
                return DeleteTarget::None;
            }
            match tab.contents {
                Contents::Remote { .. } => DeleteTarget::Remote(selected),
                Contents::Trash(_) => DeleteTarget::Trash(false),
                Contents::Tags { .. } if selected.iter().any(|r| matches!(r, Row::Tag(_))) => {
                    DeleteTarget::Tags(selected)
                }
                Contents::Directory | Contents::Tags { .. } => {
                    let paths = selected
                        .into_iter()
                        .filter_map(|row| row.path().map(Path::to_path_buf))
                        .collect();
                    DeleteTarget::Local(paths)
                }
                _ => DeleteTarget::None,
            }
        });
        match target {
            DeleteTarget::None => {}
            DeleteTarget::Remote(rows) => self.delete_remote(rows),
            DeleteTarget::Trash(all) => self.restore_trash(true, all),
            DeleteTarget::Tags(rows) => self.delete_tags(rows),
            DeleteTarget::Local(paths) => {
                if paths.is_empty() {
                    return;
                }
                let batch = match Batch::new(paths) {
                    Ok(batch) => batch,
                    Err(error) => {
                        self.report_notice(error.to_string());
                        return;
                    }
                };
                let actually_permanent = permanent || !recycle::supported();
                if actually_permanent {
                    let count = batch.paths().len();
                    let title = gds_i18n::count_untracked("delete-confirm-title", count);
                    let message = i18n::text_untracked("delete-confirm-message");
                    let primary = gds_i18n::count_untracked("delete-selected", count);
                    let browser = self.clone();
                    let paths = batch.paths().to_vec();
                    self.confirm_labelled(title, message, primary, move || {
                        if let Ok(batch) = Batch::new(paths.clone()) {
                            browser.execute_delete(batch, DeleteKind::PermanentDelete);
                        }
                    });
                } else {
                    self.execute_delete(batch, DeleteKind::Trash);
                }
            }
        }
    }

    fn execute_delete(&self, batch: Batch, kind: DeleteKind) {
        let count = batch.paths().len();
        let label = gds_i18n::count_untracked(
            if kind == DeleteKind::Trash {
                "trash-working"
            } else {
                "delete-working"
            },
            count,
        );
        let Some(cancel) = self.begin_operation(&label) else {
            return;
        };
        let affected = batch.paths().to_vec();
        let task = self.scope().spawn_blocking_named("Delete files", move || {
            batch.execute_cancellable(kind, &cancel)
        });
        let weak = self.downgrade();
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let result = task.await;
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                browser.end_operation();
                let Ok(report) = result else {
                    if let Err(error) = result {
                        browser.report_notice(error.to_string());
                    }
                    return;
                };
                let succeeded = report.succeeded_paths();
                let failed: Vec<String> = report
                    .failed_items()
                    .map(|item| {
                        let name = item.path.display().to_string();
                        match &item.result {
                            Ok(()) => name,
                            Err(err) => format!("{name}: {err}"),
                        }
                    })
                    .collect();
                if report.kind == DeleteKind::Trash && !report.receipts.is_empty() {
                    let steps = report
                        .receipts
                        .into_iter()
                        .map(history::Step::Restore)
                        .collect();
                    browser.update(|state| {
                        state
                            .history
                            .record(history::Entry::new(history::Kind::Trash, steps));
                    });
                }
                browser.refresh_after_file_operation(&affected, report.kind == DeleteKind::Trash);
                if !failed.is_empty() {
                    let headline = if report.kind == DeleteKind::Trash {
                        gds::t_untracked!(
                            "trash-partial",
                            succeeded = succeeded.len() as i64,
                            failed = failed.len() as i64
                        )
                    } else {
                        gds::t_untracked!(
                            "delete-partial",
                            succeeded = succeeded.len() as i64,
                            failed = failed.len() as i64
                        )
                    };
                    let detail = failed.join("\n");
                    browser.report_notice(format!("{headline}\n{detail}"));
                } else {
                    let key = if report.kind == DeleteKind::Trash {
                        "trash-done"
                    } else {
                        "delete-done"
                    };
                    browser.report_success(gds_i18n::count_untracked(key, succeeded.len()));
                }
                if !report.history_errors.is_empty() {
                    browser.report_notice(report.history_errors.join("; "));
                }
                browser.focus_content();
            }));
        });
    }

    pub(super) fn replay(&self, redo: bool) {
        let direction = if redo {
            history::Direction::Redo
        } else {
            history::Direction::Undo
        };
        let entry = {
            let mut state = self.0.state.borrow_mut();
            if state.operation.busy() {
                return;
            }
            state.history.take(direction)
        };
        let Some(entry) = entry else {
            return;
        };
        let op_name = i18n::text_untracked(entry.kind.label_id());
        let label = if redo {
            gds::t_untracked!("redo-operation", operation = op_name)
        } else {
            gds::t_untracked!("undo-operation", operation = op_name)
        };
        let Some(cancel) = self.begin_operation(&label) else {
            self.update(|state| {
                // Return entry cleanly if operation couldn't start.
                if redo {
                    state.history.clear();
                }
            });
            return;
        };
        let task = self
            .scope()
            .spawn_blocking_named("Replay file operation", move || {
                entry.execute_cancellable(&cancel)
            });
        let weak = self.downgrade();
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let result = task.await;
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                browser.end_operation();
                let Ok(mut report) = result else {
                    if let Err(error) = result {
                        browser.report_notice(error.to_string());
                    }
                    return;
                };
                let changed = report.changed.clone();
                let errors = report.errors.clone();
                let trash_affected = matches!(
                    report.kind,
                    history::Kind::Trash | history::Kind::Restore | history::Kind::NewFolder
                );
                browser.update(|state| {
                    state.history.finish(direction, &mut report);
                });
                browser.refresh_after_file_operation(&changed, trash_affected);
                if errors.is_empty() {
                    browser.report_success(i18n::text_untracked(if redo {
                        "redo-done"
                    } else {
                        "undo-done"
                    }));
                } else {
                    let headline =
                        i18n::text_untracked(if redo { "redo-failed" } else { "undo-failed" });
                    browser.report_notice(format!("{headline}: {}", errors.join("; ")));
                }
                browser.focus_content();
            }));
        });
    }

    pub(super) fn copy(&self, cut: bool) {
        enum CopyTarget {
            Empty,
            Remote(Vec<String>),
            Local(Vec<PathBuf>),
        }
        let target = self.read_untracked(|state| {
            let tab = state.tab();
            let selected = tab.selected();
            if selected.is_empty() {
                return CopyTarget::Empty;
            }
            if matches!(tab.contents, Contents::Remote { .. }) {
                let uris = selected
                    .into_iter()
                    .filter_map(|row| match row {
                        Row::Remote(entry) => Some(entry.address.uri().to_owned()),
                        _ => None,
                    })
                    .collect();
                return CopyTarget::Remote(uris);
            }
            let paths = selected
                .into_iter()
                .filter_map(|row| row.path().map(Path::to_path_buf))
                .collect();
            CopyTarget::Local(paths)
        });
        match target {
            CopyTarget::Empty => {}
            CopyTarget::Remote(uris) => {
                if cut {
                    self.report_notice(i18n::text_untracked("remote-cut-unavailable"));
                    return;
                }
                if uris.is_empty() {
                    return;
                }
                let count = uris.len();
                let weak = self.downgrade();
                self.0.owner.with(|| {
                    drop(spawn_local(async move {
                        let result = Clipboard::current().write_uris(uris).await;
                        let Some(browser) = weak.upgrade() else {
                            return;
                        };
                        match result {
                            Ok(()) => {
                                browser.report_success(gds_i18n::count_untracked(
                                    "copy-prepared",
                                    count,
                                ));
                                browser.changed();
                            }
                            Err(error) => browser.report_notice(error.to_string()),
                        }
                    }));
                });
            }
            CopyTarget::Local(paths) => {
                let batch = match Batch::new(paths) {
                    Ok(batch) => batch,
                    Err(error) => {
                        self.report_notice(error.to_string());
                        return;
                    }
                };
                let paths = batch.paths().to_vec();
                let count = paths.len();
                let mode = if cut {
                    clipboard::Mode::Cut
                } else {
                    clipboard::Mode::Copy
                };
                let weak = self.downgrade();
                self.0.owner.with(|| {
                    drop(spawn_local(async move {
                        let result = Clipboard::current().write(mode, paths).await;
                        let Some(browser) = weak.upgrade() else {
                            return;
                        };
                        match result {
                            Ok(_) => {
                                browser.report_success(gds_i18n::count_untracked(
                                    "copy-prepared",
                                    count,
                                ));
                                browser.changed();
                            }
                            Err(error) => browser.report_notice(error.to_string()),
                        }
                    }));
                });
            }
        }
    }

    pub(super) fn paste(&self) {
        let destination = self.read_untracked(|state| {
            if state.operation.busy() {
                return None;
            }
            let tab = state.tab();
            if !matches!(tab.contents, Contents::Directory) {
                return None;
            }
            tab.navigation.path().map(Path::to_path_buf)
        });
        let Some(destination) = destination else {
            return;
        };
        let weak = self.downgrade();
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let payload = Clipboard::current().read_payload().await;
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                match payload {
                    Ok(Payload::Empty) => {}
                    Ok(Payload::Uris { mode, uris }) => {
                        if mode == clipboard::Mode::Cut {
                            browser.report_notice(i18n::text_untracked("remote-cut-unavailable"));
                        } else {
                            browser.paste_uri_payload(uris, destination, false);
                        }
                    }
                    Ok(Payload::Local(snapshot)) => {
                        let cut = snapshot.mode == clipboard::Mode::Cut;
                        let mode = if cut {
                            transfer::Mode::Move
                        } else {
                            transfer::Mode::Copy
                        };
                        let job = PasteJob::new(destination, mode, snapshot.paths.clone());
                        browser.start_paste_job(job, cut, true, Some(snapshot));
                    }
                    Err(error) => browser.report_notice(error.to_string()),
                }
            }));
        });
    }

    pub(super) fn inspect_paste(
        &self,
        transfer: paste::PasteJob,
        cut: bool,
        history: bool,
        finish: Option<Vec<String>>,
    ) {
        let snapshot = if cut && finish.is_some() {
            Clipboard::current()
                .snapshot_untracked()
                .map(|snap| (*snap).clone())
        } else {
            None
        };
        self.start_paste_job(transfer, cut, history, snapshot);
    }

    fn start_paste_job(
        &self,
        job: PasteJob,
        cut: bool,
        record_history: bool,
        snapshot: Option<clipboard::Snapshot>,
    ) {
        let Some(cancel) = self.begin_operation(&i18n::text_untracked("paste-working")) else {
            return;
        };
        self.inspect_paste_step(job, cut, record_history, snapshot, cancel);
    }

    fn inspect_paste_step(
        &self,
        job: PasteJob,
        cut: bool,
        record_history: bool,
        snapshot: Option<clipboard::Snapshot>,
        cancel: CancellationToken,
    ) {
        let task = self
            .scope()
            .spawn_blocking_named("Inspect paste conflicts", move || job.inspect());
        let weak = self.downgrade();
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let result = task.await;
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                let Ok(inspection) = result else {
                    browser.end_operation();
                    if let Err(error) = result {
                        browser.report_notice(error.to_string());
                    }
                    return;
                };
                match inspection {
                    Inspection::Ready(ready_job) => {
                        browser.execute_paste_step(
                            ready_job,
                            cut,
                            record_history,
                            snapshot,
                            cancel,
                        );
                    }
                    Inspection::Failed { job, error } => {
                        if job.succeeded.is_empty() && job.failures.is_empty() {
                            browser.end_operation();
                            browser.report_notice(error.to_string());
                        } else {
                            let mut failed_job = job;
                            failed_job.record_abort(error.to_string());
                            browser.finish_paste_job(
                                failed_job,
                                cut,
                                record_history,
                                snapshot,
                                true,
                            );
                        }
                    }
                    Inspection::Conflict { job, conflict } => {
                        browser.ask_paste_conflict(
                            job,
                            conflict,
                            cut,
                            record_history,
                            snapshot,
                            cancel,
                        );
                    }
                }
            }));
        });
    }

    fn ask_paste_conflict(
        &self,
        job: PasteJob,
        conflict: paste::PasteConflict,
        cut: bool,
        record_history: bool,
        snapshot: Option<clipboard::Snapshot>,
        cancel: CancellationToken,
    ) {
        let title = gds::t_untracked!(
            "transfer-conflict-title",
            name = conflict.collision.target_label()
        );
        let form = NameForm::new(NamePurpose::Conflict, title, conflict.suggestion);
        let source = conflict.collision.source;
        let browser_submit = self.clone();
        let browser_cancel = self.clone();
        let shared_job = std::rc::Rc::new(std::cell::RefCell::new(Some(job)));
        let submit_job = std::rc::Rc::clone(&shared_job);
        let cancel_job = shared_job;
        let snapshot_cancel = snapshot.clone();
        self.name_form_with_cancel(
            form,
            move |chosen_name| {
                let Some(mut next_job) = submit_job.borrow_mut().take() else {
                    return;
                };
                next_job.resolve_conflict(source.clone(), chosen_name);
                browser_submit.inspect_paste_step(
                    next_job,
                    cut,
                    record_history,
                    snapshot.clone(),
                    cancel.clone(),
                );
            },
            move || {
                let Some(mut cancelled_job) = cancel_job.borrow_mut().take() else {
                    return;
                };
                cancelled_job.pending.clear();
                if cancelled_job.succeeded.is_empty() && cancelled_job.failures.is_empty() {
                    browser_cancel.end_operation();
                    browser_cancel.report_notice(i18n::text_untracked("paste-cancelled"));
                } else {
                    browser_cancel.finish_paste_job(
                        cancelled_job,
                        cut,
                        record_history,
                        snapshot_cancel.clone(),
                        false,
                    );
                }
            },
        );
    }

    fn execute_paste_step(
        &self,
        mut job: PasteJob,
        cut: bool,
        record_history: bool,
        snapshot: Option<clipboard::Snapshot>,
        cancel: CancellationToken,
    ) {
        let buffer_bytes = self
            .0
            .preferences
            .snapshot()
            .files
            .transfer
            .local_buffer_bytes();
        let batch = match job
            .prepare_batch()
            .map(|batch| batch.with_buffer_size(buffer_bytes))
        {
            Ok(batch) => batch,
            Err(error) => {
                if job.succeeded.is_empty() && job.failures.is_empty() {
                    self.end_operation();
                    self.report_notice(error.to_string());
                } else {
                    job.record_abort(error.to_string());
                    self.finish_paste_job(job, cut, record_history, snapshot, true);
                }
                return;
            }
        };

        let task_cancel = cancel.clone();
        let task = self
            .scope()
            .spawn_blocking_named("Transfer pasted files", move || {
                batch.execute_cancellable(task_cancel)
            });
        let weak = self.downgrade();
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let result = task.await;
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                let Ok(report) = result else {
                    if job.succeeded.is_empty() && job.failures.is_empty() {
                        browser.end_operation();
                        if let Err(error) = result {
                            browser.report_notice(error.to_string());
                        }
                    } else {
                        if let Err(error) = result {
                            job.record_abort(error.to_string());
                        }
                        browser.finish_paste_job(job, cut, record_history, snapshot, true);
                    }
                    return;
                };
                match job.apply_pass(report) {
                    PassOutcome::Completed(done) => {
                        browser.finish_paste_job(done, cut, record_history, snapshot, false);
                    }
                    PassOutcome::RetryConflict(next_job) => {
                        browser.inspect_paste_step(next_job, cut, record_history, snapshot, cancel);
                    }
                }
            }));
        });
    }

    fn finish_paste_job(
        &self,
        job: PasteJob,
        cut: bool,
        record_history: bool,
        snapshot: Option<clipboard::Snapshot>,
        aborted: bool,
    ) {
        let destination = job.destination.clone();
        if cut
            && !job.succeeded.is_empty()
            && let Some(snap) = snapshot
        {
            let moved = job.succeeded.clone();
            let weak = self.downgrade();
            self.0.owner.with(|| {
                drop(spawn_local(async move {
                    let clipboard_error = Clipboard::current()
                        .finish_cut(snap, moved)
                        .await
                        .err()
                        .map(|error| error.to_string());
                    let Some(browser) = weak.upgrade() else {
                        return;
                    };
                    browser.complete_paste_summary(
                        job.into_summary(clipboard_error),
                        destination,
                        record_history,
                        aborted,
                    );
                }));
            });
            return;
        }
        self.complete_paste_summary(job.into_summary(None), destination, record_history, aborted);
    }

    fn complete_paste_summary(
        &self,
        summary: paste::Summary,
        destination: PathBuf,
        record_history: bool,
        aborted: bool,
    ) {
        self.end_operation();
        let mut changed_paths = vec![destination];
        changed_paths.extend(summary.succeeded.iter().cloned());
        changed_paths.extend(summary.committed.iter().cloned());
        changed_paths.extend(summary.undo.iter().flat_map(history::Step::paths));
        if record_history {
            let kind = summary.history_kind();
            let reset = summary.requires_history_reset();
            self.update(|state| {
                if reset {
                    state.history.clear();
                }
                state
                    .history
                    .record(history::Entry::new(kind, summary.undo));
            });
        }
        self.refresh_affected(&changed_paths);
        if !summary.committed.is_empty() {
            self.reveal_paths(summary.committed);
        }

        let failed_count = summary.failures.len();
        if failed_count > 0 || summary.clipboard_error.is_some() || aborted {
            let headline = if aborted && failed_count == 0 {
                gds::t_untracked!(
                    "paste-aborted-partial",
                    completed = summary.succeeded.len() as i64
                )
            } else {
                gds::t_untracked!(
                    "transfer-partial",
                    succeeded = summary.succeeded.len() as i64,
                    failed = failed_count as i64
                )
            };
            let mut details = summary.failures;
            if let Some(err) = summary.clipboard_error {
                details.push(gds::t_untracked!("clipboard-update-failed", error = err));
            }
            let message = if details.is_empty() {
                headline
            } else {
                format!("{headline}\n{}", details.join("\n"))
            };
            self.report_notice(message);
        } else {
            let key = if summary.mode == transfer::Mode::Move {
                "move-done"
            } else {
                "copy-done"
            };
            self.report_success(gds_i18n::count_untracked(key, summary.succeeded.len()));
        }
        self.focus_content();
    }

    pub(super) fn restore_trash(&self, purge: bool, all: bool) {
        let entries: Vec<recycle::Entry> = self.read_untracked(|state| {
            if state.operation.busy() {
                return Vec::new();
            }
            let tab = state.tab();
            if !matches!(tab.contents, Contents::Trash(_)) {
                return Vec::new();
            }
            let view = tab.navigation.view();
            tab.rows
                .iter()
                .filter_map(|row| match row {
                    Row::Trash(entry) if all || view.selection.contains(&row.key()) => {
                        Some(entry.clone())
                    }
                    _ => None,
                })
                .collect()
        });
        if entries.is_empty() {
            return;
        }
        if purge {
            let count = entries.len();
            let (title, message, primary) = if all {
                (
                    i18n::text_untracked("recycle-empty-confirm-title"),
                    gds_i18n::count_untracked("recycle-empty-confirm-message", count),
                    i18n::text_untracked("recycle-empty-confirm-ok"),
                )
            } else {
                (
                    gds_i18n::count_untracked("recycle-purge-confirm-title", count),
                    i18n::text_untracked("recycle-purge-confirm-message"),
                    i18n::text_untracked("recycle-purge-confirm-ok"),
                )
            };
            let browser = self.clone();
            self.confirm_labelled(title, message, primary, move || {
                browser.execute_trash_action(recycle::Action::Purge, entries.clone());
            });
        } else {
            self.execute_trash_action(recycle::Action::Restore, entries);
        }
    }

    fn execute_trash_action(&self, action: recycle::Action, entries: Vec<recycle::Entry>) {
        let count = entries.len();
        let label = gds_i18n::count_untracked(
            match action {
                recycle::Action::Restore => "recycle-restore-working",
                recycle::Action::Purge => "recycle-purge-working",
            },
            count,
        );
        let Some(cancel) = self.begin_operation(&label) else {
            return;
        };
        let task = self
            .scope()
            .spawn_blocking_named("Trash operation", move || {
                recycle::perform_cancellable(action, entries, &cancel)
            });
        let weak = self.downgrade();
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let result = task.await;
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                browser.end_operation();
                let Ok(outcomes) = result else {
                    if let Err(error) = result {
                        browser.report_notice(error.to_string());
                    }
                    return;
                };
                let mut succeeded = 0usize;
                let mut failed = Vec::new();
                let mut undo_steps = Vec::new();
                let mut changed_paths = Vec::new();
                for outcome in outcomes {
                    match outcome.result {
                        Ok(()) => {
                            succeeded += 1;
                            if matches!(action, recycle::Action::Restore) {
                                changed_paths.push(outcome.entry.original_path.clone());
                                if let Some(snapshot) = outcome.restored {
                                    undo_steps.push(history::Step::undo_restore(
                                        &outcome.entry,
                                        snapshot,
                                    ));
                                }
                            }
                        }
                        Err(error) => {
                            failed
                                .push(format!("{}: {error}", outcome.entry.name.to_string_lossy()));
                        }
                    }
                }
                if !undo_steps.is_empty() {
                    browser.update(|state| {
                        state
                            .history
                            .record(history::Entry::new(history::Kind::Restore, undo_steps));
                    });
                }
                browser.refresh_after_file_operation(&changed_paths, true);
                if !failed.is_empty() {
                    let headline = match action {
                        recycle::Action::Restore => gds::t_untracked!(
                            "recycle-restore-partial",
                            succeeded = succeeded as i64,
                            failed = failed.len() as i64
                        ),
                        recycle::Action::Purge => gds::t_untracked!(
                            "recycle-purge-partial",
                            succeeded = succeeded as i64,
                            failed = failed.len() as i64
                        ),
                    };
                    browser.report_notice(format!("{headline}\n{}", failed.join("\n")));
                } else {
                    let key = match action {
                        recycle::Action::Restore => "recycle-restore-done",
                        recycle::Action::Purge => "recycle-purge-done",
                    };
                    browser.report_success(gds_i18n::count_untracked(key, succeeded));
                }
                browser.focus_content();
            }));
        });
    }

    pub(super) fn open_volume(&self, id: &volumes::VolumeId, new_tab: bool) {
        let Some(volume) = self.0.volumes.snapshot_untracked().find(id).cloned() else {
            self.report_notice(i18n::text_untracked("device-reconnect-missing"));
            return;
        };
        if !volume.mount_points.is_empty() {
            let location = Location::Volume {
                identity: Self::volume_identity(&volume),
                relative: OsString::new(),
            };
            if new_tab {
                self.new_tab(Some(location));
            } else {
                self.open(location);
            }
            return;
        }
        if volume.locked {
            self.report_notice(i18n::text_untracked("device-unlock-unavailable"));
            return;
        }
        if !volume.mountable {
            self.report_notice(i18n::text_untracked("device-not-mounted"));
            return;
        }
        let Ok(future) = self
            .0
            .volumes
            .operate(DeviceAction::Mount(volume.id.clone()))
        else {
            self.report_notice(i18n::text_untracked("device-path-unavailable"));
            return;
        };
        let identity = Self::volume_identity(&volume);
        let weak = self.downgrade();
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let result = future.await;
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                match result {
                    Ok(_) => {
                        let location = Location::Volume {
                            identity,
                            relative: OsString::new(),
                        };
                        if new_tab {
                            browser.new_tab(Some(location));
                        } else {
                            browser.open(location);
                        }
                    }
                    Err(error) => browser.report_notice(error.to_string()),
                }
            }));
        });
    }

    pub(super) fn reconnect_volume(&self) {
        let location = self.read_untracked(|state| state.tab().navigation.location().clone());
        let Location::Volume { ref identity, .. } = location else {
            self.refresh();
            return;
        };
        self.0.volumes.refresh();
        if let Some(volume) = self.volume_at_location_untracked(identity) {
            if !volume.mount_points.is_empty() {
                self.refresh();
            } else {
                self.open_volume(&volume.id, false);
            }
        } else {
            self.report_notice(i18n::text_untracked("device-reconnect-missing"));
        }
    }

    pub(super) fn select_context(&self, index: usize) {
        let changed = self.update(|state| {
            let tab = state.tab_mut();
            let Some(row) = tab.rows.get(index) else {
                return false;
            };
            let key = row.key();
            let view = tab.navigation.view_mut();
            if view.selection.contains(&key) {
                view.cursor = Some(key);
                return false;
            }
            view.selection.replace([key.clone()]);
            view.anchor = Some(key.clone());
            view.cursor = Some(key);
            true
        });
        if changed {
            self.update_selection(|_| {});
        }
    }

    pub(super) fn column_sort(&self, column: Column) {
        self.sort_by(column);
    }
}

trait SnapshotExt {
    fn find(&self, id: &volumes::VolumeId) -> Option<&volumes::Volume>;
}

impl SnapshotExt for volumes::Snapshot {
    fn find(&self, id: &volumes::VolumeId) -> Option<&volumes::Volume> {
        self.volumes.iter().find(|volume| &volume.id == id)
    }
}
