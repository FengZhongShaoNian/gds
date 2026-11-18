#![allow(dead_code)]

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use zgui::prelude::*;

use super::ui::Browser;
use crate::{
    entities::{
        file::{columns::Column, directory},
        location::{Location, VolumeIdentity, volumes},
        tags,
    },
    features::file_operations::recycle,
};

pub(super) enum Loaded {
    Unavailable(String),
    Directory(Option<Arc<directory::DirectorySnapshot>>),
    Places(Vec<Location>),
    Network(Vec<PathBuf>),
    Trash(Vec<recycle::Entry>),
    Tags {
        listing: tags::Listing,
        backend: tags::Baloo,
        entries: Vec<directory::Entry>,
    },
    Volumes,
}

impl Browser {
    pub(super) fn resolve_path(&self, location: &Location) -> Option<PathBuf> {
        match location {
            Location::Directory(path) => Some(path.into()),
            Location::Volume { identity, relative } => {
                let volume = self.volume_at_location(identity)?;
                let preferred = volume.id.stable.as_ref().and_then(|id| {
                    self.0
                        .preferences
                        .snapshot()
                        .files
                        .volume_mount_points
                        .get(id)
                        .cloned()
                });
                preferred
                    .as_ref()
                    .filter(|preferred| volume.mount_points.contains(preferred))
                    .or_else(|| {
                        volume.mount_points.iter().min_by(|left, right| {
                            left.components().count().cmp(&right.components().count())
                        })
                    })
                    .map(|mount| {
                        if relative.as_os_str().is_empty() {
                            mount.clone()
                        } else {
                            mount.join(relative)
                        }
                    })
            }
            _ => None,
        }
    }

    pub(super) fn volume_at_location(&self, identity: &VolumeIdentity) -> Option<volumes::Volume> {
        let snapshot = self.0.volumes.snapshot();
        snapshot.volumes.into_iter().find(|volume| match identity {
            VolumeIdentity::Stable(stable) => volume.id.stable.as_ref() == Some(stable),
            VolumeIdentity::Attachment(attachment) => volume.id.attachment == *attachment,
        })
    }

    pub(super) fn volume_identity(volume: &volumes::Volume) -> VolumeIdentity {
        if let Some(stable) = &volume.id.stable {
            VolumeIdentity::Stable(stable.clone())
        } else {
            VolumeIdentity::Attachment(volume.id.attachment.clone())
        }
    }

    pub(super) fn location_title(&self, location: &Location) -> String {
        location.label()
    }

    pub(super) fn location_address(location: &Location, _path: Option<&Path>) -> String {
        location.display_address()
    }

    pub(super) fn local_location(&self, path: &Path) -> Location {
        Location::from(path)
    }

    pub(super) fn open(&self, location: Location) {
        let id = self.read(|state| state.tab().id);
        self.open_in_tab(id, location);
    }

    pub(super) fn open_in_tab(&self, _id: u64, _location: Location) {}

    pub(super) fn open_address(&self) {}

    pub(super) fn refresh(&self) {
        let id = self.read(|state| state.tab().id);
        self.refresh_tab(id);
    }

    pub(super) fn reveal_paths(&self, paths: Vec<PathBuf>) {
        self.update(|state| state.tab_mut().reveal = paths);
    }

    pub(super) fn refresh_tab(&self, _id: u64) {}

    pub(super) fn history(&self, _forward: bool) {}

    pub(super) fn visit_history(&self, _id: u64, _index: usize, _location: &Location) {}

    pub(super) fn cancel_location_load(&self) {}

    pub(super) fn parent(&self) {}

    pub(super) fn new_tab(&self, _location: Option<Location>) {}

    pub(super) fn select_tab(&self, index: usize) {
        self.update(|state| {
            if index < state.tabs.len() {
                state.active = index;
            }
        });
    }

    pub(super) fn tab_index(&self, id: u64) -> Option<usize> {
        self.read(|state| state.tabs.iter().position(|tab| tab.id == id))
    }

    pub(super) fn close_tab(&self, _index: usize) {}

    pub(super) fn toggle_pin(&self, _index: usize) {}

    pub(super) fn reorder_tab(&self, _from: usize, _to: usize) {}

    pub(super) fn sortable_columns(&self) -> Vec<Column> {
        Vec::new()
    }

    pub(super) fn reproject(&self, _id: u64) {}

    pub(super) fn sync_address(&self) {
        let address = self.read(|state| state.tab().navigation.location().display_address());
        self.0.address.set(address);
    }

    pub(super) fn filter(&self, value: String) {
        self.0.filter.set(value);
    }

    pub(super) fn toggle_view(&self) {}

    pub(super) fn favorite(&self) {}
}
