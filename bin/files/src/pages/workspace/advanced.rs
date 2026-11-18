#![allow(dead_code)]
//! Remote sessions, catalogue/device operations, and desktop requests.

use std::path::PathBuf;

use gds::service::{
    desktop::file_manager::{Command, Request},
    remote,
};
use zgui::prelude::*;

use super::{model::Row, ui::Browser};
use crate::entities::location::{Location, navigation::LoadRequest, volumes};

impl Browser {
    pub(super) fn connect_server(&self) {}

    pub(super) fn load_remote(&self, _id: u64, _request: LoadRequest, _address: remote::Address) {}

    pub(super) fn activate_remote(&self, _entry: remote::Entry, _new_tab: bool) {}

    pub(super) fn download_remote(&self, _addresses: Vec<remote::Address>, _open: bool) {}

    pub(super) fn upload_remote(&self) {}

    pub(super) fn download_selected_remote(&self) {}

    pub(super) fn disconnect_remote(&self) {}

    pub(super) fn rename_remote(&self, _entry: remote::Entry) {}

    pub(super) fn paste_uris(&self) {}

    pub(super) fn paste_uri_payload(&self, _uris: Vec<String>, _destination: PathBuf, _cut: bool) {}

    pub(super) fn delete_remote(&self, _rows: Vec<Row>) {}

    pub(super) fn delete_tags(&self, _rows: Vec<Row>) {}

    pub(super) fn refresh_tag_tabs(&self) {}

    pub(super) fn refresh_location(&self, _location: &Location) {}

    pub(super) fn rename_volume(&self, _volume: volumes::Volume) {}

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
