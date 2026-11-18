#![allow(dead_code)]

use std::path::PathBuf;

use gds::service::desktop::OpenMode;
use tokio_util::sync::CancellationToken;

use super::{
    model::{Operation, Origin},
    ui::Browser,
};
use crate::entities::{file::columns::Column, location::volumes};

impl Browser {
    pub(super) fn origin(&self) -> Origin {
        self.read(|state| Origin {
            tab: state.tab().id,
            location: state.tab().navigation.location().clone(),
        })
    }

    pub(super) fn refresh_affected(&self, _paths: &[PathBuf]) {}

    pub(super) fn begin_operation(&self, label: &str) -> Option<CancellationToken> {
        let cancel = CancellationToken::new();
        let origin = self.origin();
        let started = self.update(|state| {
            if state.operation.busy() {
                return false;
            }
            state.operation = Operation::Running {
                label: label.to_owned(),
                cancel: cancel.clone(),
                origin,
            };
            true
        });
        started.then_some(cancel)
    }

    pub(super) fn end_operation(&self) {
        self.update(|state| state.operation = Operation::Idle);
    }

    pub(super) fn open_selected(&self) {}

    pub(super) fn open_row(&self, _index: usize, _new_tab: bool) {}

    pub(super) fn open_file(&self, _path: PathBuf, _mode: OpenMode) {}

    pub(super) fn new_folder(&self) {}

    pub(super) fn rename(&self) {}

    pub(super) fn delete(&self, _permanent: bool) {}

    pub(super) fn replay(&self, _redo: bool) {}

    pub(super) fn copy(&self, _cut: bool) {}

    pub(super) fn paste(&self) {}

    pub(super) fn inspect_paste(
        &self,
        _transfer: crate::features::file_operations::paste::PasteJob,
        _cut: bool,
        _history: bool,
        _finish: Option<Vec<String>>,
    ) {
    }

    pub(super) fn restore_trash(&self, _purge: bool, _all: bool) {}

    pub(super) fn open_volume(&self, _id: &volumes::VolumeId, _new_tab: bool) {}

    pub(super) fn reconnect_volume(&self) {}

    pub(super) fn select_context(&self, _index: usize) {}

    pub(super) fn column_sort(&self, column: Column) {
        self.sort_by(column);
    }
}
