//! Remote sessions, catalogue/device operations, and desktop requests.

use std::{
    cell::RefCell,
    ffi::OsString,
    path::{Path, PathBuf},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use gds::app::Gds;
use gds::i18n as gds_i18n;
use gds::service::{
    desktop::{
        OpenMode,
        file_manager::{Command, Request},
    },
    remote::{self, EntryKind},
};
use gds::t;
use tokio::sync::{mpsc, watch};
use zgui::prelude::*;
use zgui::reactive::spawn_local;
use zgui::view;

use super::{
    model::{Contents, Row},
    name_form::{NameForm, NamePurpose},
    navigation::Loaded,
    ui::{Browser, LaunchOptions},
};
use crate::{
    entities::{
        location::{
            Location,
            navigation::{LoadOutcome, LoadRequest},
            places::Places,
            volumes::{self, DeviceAction},
        },
        tags,
    },
    features::{
        connect_server::Intent,
        file_operations::{
            history,
            paste::PasteJob,
            remote::{self as remote_transfer, Resource},
            transfer,
        },
        pick_files,
    },
    shared::{
        i18n,
        lib::clipboard::{Clipboard, Payload},
    },
};

impl Browser {
    #[allow(dead_code)]
    pub(super) fn connect_server(&self) {
        let browser = self.clone();
        self.0
            .connector
            .prompt(None, Intent::Open, move |result| match result {
                Ok(address) => browser.open(Location::Remote(address)),
                Err(error) => browser.report_notice(error),
            });
    }

    pub(super) fn load_remote(&self, id: u64, request: LoadRequest, address: remote::Address) {
        let Some(session) = self.0.connector.session(&address) else {
            let prompt_browser = self.clone();
            let prompt_request = request.clone();
            let prompt_address = address.clone();
            self.0
                .connector
                .prompt(Some(address), Intent::Open, move |result| match result {
                    Ok(connected) => {
                        prompt_browser.load_remote(id, prompt_request.clone(), connected);
                    }
                    Err(error) => {
                        prompt_browser.finish_collection_load(
                            id,
                            &prompt_request,
                            Loaded::Unavailable(error),
                        );
                    }
                });
            let _ = &prompt_address;
            return;
        };

        let work = self.0.tasks.named_child("Load remote folder");
        let scope = (*work).clone();
        {
            let mut state = self.0.state.borrow_mut();
            let Some(tab) = state.tab_by_id(id) else {
                return;
            };
            if !matches!(tab.contents, Contents::Remote { .. }) {
                tab.contents = Contents::LoadingCollection;
                tab.rows.clear();
            }
            tab.work = Some(work);
        }
        self.changed();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let list_address = address.clone();
        let list_session = session.clone();
        drop(scope.spawn_named("List remote directory", async move {
            let result = list_session.list(&list_address).await;
            let _ = tx.send(result);
        }));

        let weak = self.downgrade();
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let Some(result) = rx.recv().await else {
                    return;
                };
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                match result {
                    Ok(listing) => {
                        let applied = browser.update(|state| {
                            let Some(tab) = state.tab_by_id(id) else {
                                return false;
                            };
                            match tab.navigation.finish_load(&request, Ok(None)) {
                                Ok(LoadOutcome::Applied) => {
                                    tab.work = None;
                                    tab.enrichment = None;
                                    tab.contents = Contents::Remote { listing, session };
                                    state.places.record(Location::Remote(address));
                                    true
                                }
                                Ok(LoadOutcome::Stale) | Err(_) => false,
                            }
                        });
                        if applied {
                            browser.save_places();
                            browser.sync_address();
                            browser.reproject(id);
                        }
                    }
                    Err(error) => {
                        browser.finish_collection_load(
                            id,
                            &request,
                            Loaded::Unavailable(error.to_string()),
                        );
                    }
                }
            }));
        });
    }

    pub(super) fn activate_remote(&self, entry: remote::Entry, new_tab: bool) {
        let in_picker = self.read(|state| state.picker.is_some());
        match entry.kind {
            EntryKind::Directory => {
                let location = Location::Remote(entry.address);
                if new_tab && !in_picker {
                    self.new_tab(Some(location));
                } else {
                    self.open(location);
                }
            }
            EntryKind::File => {
                if in_picker {
                    self.report_notice(i18n::text("remote-picker-local"));
                } else {
                    self.download_remote(vec![entry.address], true);
                }
            }
            EntryKind::SymbolicLink | EntryKind::Other => {
                if in_picker {
                    self.report_notice(i18n::text("remote-picker-local"));
                } else {
                    self.download_remote(vec![entry.address], true);
                }
            }
        }
    }

    pub(super) fn download_remote(&self, addresses: Vec<remote::Address>, open: bool) {
        if addresses.is_empty() || self.read(|state| state.operation.busy()) {
            return;
        }
        let Some(first) = addresses.first() else {
            return;
        };
        let session = self
            .read(|state| match &state.tab().contents {
                Contents::Remote { session, .. } => Some(session.clone()),
                _ => None,
            })
            .or_else(|| self.0.connector.session(first));
        let Some(session) = session else {
            self.report_notice(i18n::text("remote-connect-source"));
            return;
        };

        if open && addresses.len() == 1 {
            let addr = addresses[0].clone();
            let preview_dir = remote_preview_dir();
            if let Err(error) = std::fs::create_dir_all(&preview_dir) {
                self.report_notice(error.to_string());
                return;
            }
            let expected_local = preview_dir.join(addr.name());
            self.run_remote_copy(
                vec![Resource::Remote {
                    address: addr,
                    session,
                }],
                Resource::Local(preview_dir),
                false,
                Some(expected_local),
            );
            return;
        }

        let browser = self.clone();
        self.open_transfer_picker(
            i18n::text("remote-download"),
            pick_files::Options {
                directory: true,
                multiple: false,
                ..pick_files::Options::new(Vec::new(), false)
            },
            move |paths| {
                let Some(dest) = paths.into_iter().next() else {
                    return;
                };
                let sources: Vec<Resource> = addresses
                    .iter()
                    .cloned()
                    .map(|address| Resource::Remote {
                        address,
                        session: session.clone(),
                    })
                    .collect();
                browser.run_remote_copy(sources, Resource::Local(dest), false, None);
            },
        );
    }

    #[allow(dead_code)]
    pub(super) fn upload_remote(&self) {
        let target = self.read(|state| match &state.tab().contents {
            Contents::Remote { listing, session } => {
                Some((listing.address.clone(), session.clone()))
            }
            _ => None,
        });
        let Some((dest_address, session)) = target else {
            return;
        };
        if self.read(|state| state.operation.busy()) {
            return;
        }
        let browser = self.clone();
        self.open_transfer_picker(
            i18n::text("remote-upload"),
            pick_files::Options {
                allow_mixed: true,
                multiple: true,
                ..pick_files::Options::new(Vec::new(), false)
            },
            move |paths| {
                if paths.is_empty() {
                    return;
                }
                let sources = paths.into_iter().map(Resource::Local).collect();
                browser.run_remote_copy(
                    sources,
                    Resource::Remote {
                        address: dest_address.clone(),
                        session: session.clone(),
                    },
                    false,
                    None,
                );
            },
        );
    }

    #[allow(dead_code)]
    pub(super) fn download_selected_remote(&self) {
        let addresses: Vec<remote::Address> = self.read(|state| {
            state
                .tab()
                .selected()
                .into_iter()
                .filter_map(|row| match row {
                    Row::Remote(entry) => Some(entry.address),
                    _ => None,
                })
                .collect()
        });
        self.download_remote(addresses, false);
    }

    #[allow(dead_code)]
    pub(super) fn disconnect_remote(&self) {
        let address = self.read(|state| {
            let tab = state.tab();
            match tab.navigation.location() {
                Location::Remote(address) => Some(address.clone()),
                _ => tab.selected().into_iter().find_map(|row| match row {
                    Row::Place(Location::Remote(address)) => Some(address),
                    _ => None,
                }),
            }
        });
        let Some(address) = address else {
            return;
        };
        let browser = self.clone();
        self.0
            .connector
            .disconnect(address, move |result| match result {
                Ok(()) => {
                    let on_remote = browser.read(|state| {
                        matches!(state.tab().navigation.location(), Location::Remote(_))
                    });
                    if on_remote {
                        browser.open(Location::Network);
                    } else {
                        browser.refresh_location(&Location::Network);
                    }
                }
                Err(error) => browser.report_notice(error),
            });
    }

    pub(super) fn rename_remote(&self, entry: remote::Entry) {
        let session = self.read(|state| match &state.tab().contents {
            Contents::Remote { session, .. } => Some(session.clone()),
            _ => None,
        });
        let Some(session) = session else {
            return;
        };
        if self.read(|state| state.operation.busy()) {
            return;
        }
        let from = entry.address.clone();
        let current_name = entry.name.clone();
        let form = NameForm::new(
            NamePurpose::Rename,
            t!("rename-title", name = current_name.clone()),
            OsString::from(&current_name),
        );
        let browser = self.clone();
        self.name_form(form, move |new_os_name| {
            let Some(new_name) = new_os_name.to_str() else {
                browser.report_notice(i18n::text("rename-not-text"));
                return;
            };
            if new_name == current_name {
                return;
            }
            let Ok(target) = renamed_remote_address(&from, new_name) else {
                browser.report_notice(i18n::text("rename-error-invalid"));
                return;
            };
            let Some(_cancel) = browser.begin_operation(&i18n::text("rename-working")) else {
                return;
            };
            let session = session.clone();
            let from = from.clone();
            let display_name = new_name.to_owned();
            let (tx, mut rx) = mpsc::unbounded_channel();
            drop(
                browser
                    .scope()
                    .spawn_named("Rename remote item", async move {
                        let result = session.rename(&from, &target).await;
                        let _ = tx.send(result);
                    }),
            );
            let weak = browser.downgrade();
            browser.0.owner.with(|| {
                drop(spawn_local(async move {
                    let Some(result) = rx.recv().await else {
                        return;
                    };
                    let Some(browser) = weak.upgrade() else {
                        return;
                    };
                    browser.end_operation();
                    match result {
                        Ok(()) => {
                            browser.refresh();
                            browser.report_success(t!("rename-done", name = display_name));
                        }
                        Err(error) => {
                            browser.report_notice(t!("rename-failed", error = error.to_string()));
                        }
                    }
                    browser.focus_content();
                }));
            });
        });
    }

    #[allow(dead_code)]
    pub(super) fn paste_uris(&self) {
        let destination = self.read(|state| {
            if state.operation.busy() {
                return None;
            }
            let tab = state.tab();
            matches!(tab.contents, Contents::Directory)
                .then(|| tab.navigation.path().map(Path::to_path_buf))
                .flatten()
        });
        let Some(destination) = destination else {
            return;
        };
        let weak = self.downgrade();
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let result = Clipboard::current().read_payload().await;
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                match result {
                    Ok(Payload::Uris { mode, uris }) => {
                        browser.paste_uri_payload(
                            uris,
                            destination,
                            mode == crate::shared::lib::clipboard::Mode::Cut,
                        );
                    }
                    Ok(Payload::Local(_)) => browser.paste(),
                    Ok(Payload::Empty) => {}
                    Err(error) => browser.report_notice(error.to_string()),
                }
            }));
        });
    }

    pub(super) fn paste_uri_payload(&self, uris: Vec<String>, destination: PathBuf, cut: bool) {
        if cut {
            self.report_notice(i18n::text("remote-cut-unavailable"));
            return;
        }
        let parsed = classify_uri_payload(&uris);
        if !parsed.local.is_empty() {
            let job = PasteJob::new(destination.clone(), transfer::Mode::Copy, parsed.local);
            self.inspect_paste(job, false, true, None);
        }
        if !parsed.remote.is_empty() {
            let mut resources = Vec::with_capacity(parsed.remote.len());
            for address in parsed.remote {
                let Some(session) = self.0.connector.session(&address) else {
                    self.report_notice(i18n::text("remote-connect-source"));
                    return;
                };
                resources.push(Resource::Remote { address, session });
            }
            self.run_remote_copy(resources, Resource::Local(destination), true, None);
        }
    }

    pub(super) fn delete_remote(&self, rows: Vec<Row>) {
        if self.read(|state| state.operation.busy()) {
            return;
        }
        let session = self.read(|state| match &state.tab().contents {
            Contents::Remote { session, .. } => Some(session.clone()),
            _ => None,
        });
        let Some(session) = session else {
            return;
        };
        let resources: Vec<Resource> = rows
            .into_iter()
            .filter_map(|row| match row {
                Row::Remote(entry) => Some(Resource::Remote {
                    address: entry.address,
                    session: session.clone(),
                }),
                _ => None,
            })
            .collect();
        if resources.is_empty() {
            return;
        }
        let count = resources.len();
        let title = gds_i18n::count("delete-confirm-title", count);
        let message = i18n::text("remote-delete-warning");
        let primary = gds_i18n::count("delete-selected", count);
        let browser = self.clone();
        self.confirm_labelled(title, message, primary, move || {
            let Some(cancel) = browser.begin_operation(&gds_i18n::count("delete-working", count))
            else {
                return;
            };
            let to_delete = resources.clone();
            let (tx, mut rx) = mpsc::unbounded_channel();
            let (updates_tx, _updates_rx) = watch::channel(remote_transfer::Progress::default());
            drop(
                browser
                    .scope()
                    .spawn_named("Delete remote files", async move {
                        let report = remote_transfer::delete(to_delete, cancel, updates_tx).await;
                        let _ = tx.send(report);
                    }),
            );
            let weak = browser.downgrade();
            browser.0.owner.with(|| {
                drop(spawn_local(async move {
                    let Some(report) = rx.recv().await else {
                        return;
                    };
                    let Some(browser) = weak.upgrade() else {
                        return;
                    };
                    browser.end_operation();
                    browser.refresh();
                    if report.cancelled {
                        browser.report_notice(i18n::text("remote-cancelled"));
                    } else if !report.failures.is_empty() {
                        let headline = t!(
                            "delete-partial",
                            succeeded = report.completed.len() as i64,
                            failed = report.failures.len() as i64
                        );
                        let details: Vec<String> = report
                            .failures
                            .into_iter()
                            .map(|(item, err)| format!("{item}: {err}"))
                            .collect();
                        browser.report_notice(format!("{headline}\n{}", details.join("\n")));
                    } else {
                        browser
                            .report_success(gds_i18n::count("delete-done", report.completed.len()));
                    }
                    browser.focus_content();
                }));
            });
        });
    }

    pub(super) fn delete_tags(&self, rows: Vec<Row>) {
        if self.read(|state| state.operation.busy()) {
            return;
        }
        let tags_to_delete: Vec<String> = rows
            .into_iter()
            .filter_map(|row| match row {
                Row::Tag(tag) => Some(tag),
                _ => None,
            })
            .collect();
        if tags_to_delete.is_empty() {
            return;
        }
        let backend = self.read(|state| match &state.tab().contents {
            Contents::Tags { backend, .. } => Some(backend.clone()),
            _ => None,
        });
        let commands = self.0.preferences.snapshot().tags.commands;
        let count = tags_to_delete.len();
        let title = gds_i18n::count("delete-confirm-title", count);
        let message = i18n::text("tags-delete-message");
        let primary = gds_i18n::count("delete-selected", count);
        let browser = self.clone();
        self.confirm_labelled(title, message, primary, move || {
            let Some(_cancel) = browser.begin_operation(&gds_i18n::count("delete-working", count))
            else {
                return;
            };
            let tags_list = tags_to_delete.clone();
            let existing_backend = backend.clone();
            let cmds = commands.clone();
            let (tx, mut rx) = mpsc::unbounded_channel();
            drop(browser.scope().spawn_named("Delete file tags", async move {
                let backend = match existing_backend {
                    Some(backend) => Ok(backend),
                    None => tags::connect(&cmds).await,
                };
                let outcome = match backend {
                    Ok(backend) => {
                        let mut deleted = 0usize;
                        let mut failures = Vec::new();
                        for tag in tags_list {
                            match backend.delete_tag(&tag).await {
                                Ok(()) => deleted += 1,
                                Err(err) => failures.push(format!("{tag}: {err}")),
                            }
                        }
                        Ok((deleted, failures))
                    }
                    Err(err) => Err(err),
                };
                let _ = tx.send(outcome);
            }));
            let weak = browser.downgrade();
            browser.0.owner.with(|| {
                drop(spawn_local(async move {
                    let Some(outcome) = rx.recv().await else {
                        return;
                    };
                    let Some(browser) = weak.upgrade() else {
                        return;
                    };
                    browser.end_operation();
                    match outcome {
                        Ok((deleted, failures)) => {
                            browser.refresh_tag_tabs();
                            if !failures.is_empty() {
                                let headline = t!(
                                    "delete-partial",
                                    succeeded = deleted as i64,
                                    failed = failures.len() as i64
                                );
                                browser
                                    .report_notice(format!("{headline}\n{}", failures.join("\n")));
                            } else {
                                browser.report_success(gds_i18n::count("delete-done", deleted));
                            }
                        }
                        Err(err) => browser.report_notice(err),
                    }
                    browser.focus_content();
                }));
            });
        });
    }

    pub(super) fn refresh_tag_tabs(&self) {
        let ids: Vec<u64> = self.read(|state| {
            state
                .tabs
                .iter()
                .filter(|tab| matches!(tab.navigation.location(), Location::Tags(_)))
                .map(|tab| tab.id)
                .collect()
        });
        for id in ids {
            self.refresh_tab(id);
        }
    }

    #[allow(dead_code)]
    pub(super) fn refresh_location(&self, location: &Location) {
        let ids: Vec<u64> = self.read(|state| {
            state
                .tabs
                .iter()
                .filter(|tab| tab.navigation.location() == location)
                .map(|tab| tab.id)
                .collect()
        });
        for id in ids {
            self.refresh_tab(id);
        }
    }

    pub(super) fn rename_volume(&self, volume: volumes::Volume) {
        let browser = self.clone();
        let volume_id = volume.id.clone();
        self.prompt_name(
            i18n::text("volume-rename"),
            volume.label,
            move |new_label| {
                let Ok(future) = browser.0.volumes.operate(DeviceAction::SetLabel {
                    volume: volume_id.clone(),
                    label: new_label,
                }) else {
                    browser.report_notice(i18n::text("device-path-unavailable"));
                    return;
                };
                let weak = browser.downgrade();
                browser.0.owner.with(|| {
                    drop(spawn_local(async move {
                        let result = future.await;
                        let Some(browser) = weak.upgrade() else {
                            return;
                        };
                        match result {
                            Ok(_) => {
                                browser.0.volumes.refresh();
                                browser.report_success(i18n::text("volume-renamed"));
                            }
                            Err(error) => browser.report_notice(error.to_string()),
                        }
                    }));
                });
            },
        );
    }

    fn open_transfer_picker(
        &self,
        title: String,
        options: pick_files::Options,
        on_picked: impl Fn(Vec<PathBuf>) + 'static,
    ) {
        let Some(windows) = try_use_windows() else {
            return;
        };
        let (snapshot, home) = self.read(|state| ((*state.places).clone(), state.home.clone()));
        let mut places = Places::default();
        places.accept(snapshot);
        let launch = LaunchOptions {
            places: Ok(places),
            start: home.clone(),
            home,
            picker: Some(options),
        };
        let launch_slot = RefCell::new(Some(launch));
        let callback: Rc<dyn Fn(Vec<PathBuf>)> = Rc::new(on_picked);
        let volumes = self.0.volumes.clone();
        let new_window = Rc::clone(&self.0.new_window);
        let window_opts = gds::ui::window::workspace_options(title, None);
        windows.open(window_opts, move || {
            let _ = Gds::current().appearance().install_window_sheet();
            let Some(launch) = launch_slot.borrow_mut().take() else {
                return AnyView::new(view! { box() });
            };
            let browser = Browser::new_with_picker_result(
                launch,
                volumes.clone(),
                Rc::clone(&new_window),
                Rc::clone(&callback),
            );
            AnyView::new(browser.view())
        });
    }

    fn run_remote_copy(
        &self,
        sources: Vec<Resource>,
        destination: Resource,
        record_history: bool,
        open_after: Option<PathBuf>,
    ) {
        let count = sources.len();
        let Some(cancel) = self.begin_operation(&gds_i18n::count("transfer-working", count)) else {
            return;
        };
        let buffer_size = self
            .0
            .preferences
            .snapshot()
            .files
            .transfer
            .remote_buffer_bytes();
        let scope = self.scope();
        let spawn_scope = scope.clone();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (updates_tx, _updates_rx) = watch::channel(remote_transfer::Progress::default());
        drop(spawn_scope.spawn_named("Remote file transfer", async move {
            let report = remote_transfer::copy(
                scope,
                sources,
                destination,
                remote_transfer::CopyOptions { buffer_size },
                cancel,
                updates_tx,
            )
            .await;
            let _ = tx.send(report);
        }));

        let weak = self.downgrade();
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let Some(report) = rx.recv().await else {
                    return;
                };
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                browser.end_operation();
                if record_history && !report.undo.is_empty() {
                    browser.update(|state| {
                        state
                            .history
                            .record(history::Entry::new(history::Kind::Copy, report.undo));
                    });
                }
                browser.refresh();
                if report.cancelled {
                    browser.report_notice(i18n::text("remote-cancelled"));
                } else if !report.failures.is_empty() {
                    let headline = t!(
                        "transfer-partial",
                        succeeded = report.completed.len() as i64,
                        failed = report.failures.len() as i64
                    );
                    let details: Vec<String> = report
                        .failures
                        .into_iter()
                        .map(|(item, err)| format!("{item}: {err}"))
                        .collect();
                    browser.report_notice(format!("{headline}\n{}", details.join("\n")));
                } else if let Some(path) = open_after {
                    browser.open_file(path, OpenMode::DefaultApplication);
                } else {
                    browser.report_success(gds_i18n::count("copy-done", report.completed.len()));
                }
                browser.focus_content();
            }));
        });
    }

    pub(crate) fn apply_desktop_command(&self, command: Command) {
        if self.read(|state| state.picker.is_some()) {
            return;
        }
        let properties = matches!(command.request, Request::ShowProperties { .. });
        let targets = match command.request {
            Request::NewWindow { directory } => Target::folders(vec![directory]),
            Request::ShowFolders { directories } => Target::folders(directories),
            Request::ShowItems { paths } | Request::ShowProperties { paths } => {
                Target::items(paths)
            }
        };
        if properties {
            for path in targets.iter().flat_map(|target| target.selection.iter()) {
                super::inspection::Properties::open(path.clone(), self.scope());
            }
        }
        self.show_files();
        for target in targets {
            let existing = self.read(|state| {
                state
                    .tabs
                    .iter()
                    .position(|tab| tab.navigation.path() == Some(target.directory.as_path()))
            });
            if let Some(index) = existing {
                self.select_tab(index);
            } else {
                self.new_tab(Some(Location::from(target.directory.as_path())));
            }
            self.update(|state| state.tab_mut().navigation.view_mut().filter.clear());
            self.reveal_paths(target.selection);
            self.sync_address();
            self.refresh();
            if properties {
                self.0.details.set(true);
            }
        }
        self.focus_content();
        if !command.startup_id.is_empty()
            && let Some(window) = &self.0.window
        {
            gds::ui::window::activate_with_token(window, &command.startup_id);
        }
    }
}

struct UriPayload {
    local: Vec<PathBuf>,
    remote: Vec<remote::Address>,
}

fn classify_uri_payload(uris: &[String]) -> UriPayload {
    let mut local = Vec::new();
    let mut remote = Vec::new();
    for raw in uris {
        match Location::from_uri(raw) {
            Ok(Location::Directory(path)) => local.push(PathBuf::from(path)),
            Ok(Location::Remote(address)) => remote.push(address),
            _ => {
                let candidate = Path::new(raw);
                if candidate.is_absolute() {
                    local.push(candidate.to_path_buf());
                }
            }
        }
    }
    UriPayload { local, remote }
}

fn renamed_remote_address(
    address: &remote::Address,
    new_name: &str,
) -> Result<remote::Address, remote::Error> {
    remote::Address::validate_name(new_name, address.protocol())?;
    if let Some(parent) = address.parent() {
        parent.child(new_name)
    } else {
        address.child(new_name)
    }
}

fn remote_preview_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!("gds-remote-{stamp}"))
}

struct Target {
    directory: PathBuf,
    selection: Vec<PathBuf>,
}

#[cfg(test)]
mod tests;

impl Target {
    fn folders(directories: Vec<PathBuf>) -> Vec<Self> {
        let mut targets = Vec::<Self>::new();
        for directory in directories {
            if !targets.iter().any(|target| target.directory == directory) {
                targets.push(Self {
                    directory,
                    selection: Vec::new(),
                });
            }
        }
        targets
    }

    fn items(paths: Vec<PathBuf>) -> Vec<Self> {
        let mut targets = Vec::<Self>::new();
        for path in paths {
            let directory = path.parent().unwrap_or(&path).to_owned();
            if let Some(target) = targets
                .iter_mut()
                .find(|target| target.directory == directory)
            {
                if !target.selection.contains(&path) {
                    target.selection.push(path);
                }
            } else {
                targets.push(Self {
                    directory,
                    selection: vec![path],
                });
            }
        }
        targets
    }
}
