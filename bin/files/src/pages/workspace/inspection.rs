#![allow(dead_code)]
//! Properties and content inspection share the existing filesystem services.

mod info;
pub(super) mod tags;

use std::path::PathBuf;

use gds::service::task_manager::Scope;
use zgui::prelude::*;
use zgui::view;

use super::{model::Row, ui::Browser};
use crate::entities::file::directory::Entry;

#[derive(Clone)]
pub(crate) struct Properties {
    path: PathBuf,
    scope: Scope,
}

impl Properties {
    pub(crate) fn new(path: PathBuf, scope: Scope) -> Self {
        Self { path, scope }
    }

    pub(crate) fn open(path: PathBuf, scope: Scope) {
        let _ = Self::new(path, scope);
    }
}

impl Browser {
    pub(super) fn edit_tags(&self, _path: PathBuf) {}

    pub(super) fn tags_dialog(&self) -> AnyView {
        AnyView::new(view! { box() })
    }

    pub(super) fn edit_permissions(&self, _entry: Entry) {}

    pub(super) fn close_permissions(&self) {}

    pub(super) fn permissions_dialog(&self) -> AnyView {
        AnyView::new(view! { box() })
    }

    pub(super) fn row_artwork_with_selection(
        &self,
        row: &Row,
        size: f32,
        selected: Signal<f32, LocalStorage>,
    ) -> AnyView {
        self.artwork(row, size, selected)
    }

    pub(super) fn artwork(
        &self,
        _row: &Row,
        _size: f32,
        _selected: Signal<f32, LocalStorage>,
    ) -> AnyView {
        AnyView::new(view! { box() })
    }

    pub(super) fn toggle_details(&self) {
        self.0.details.update(|visible| *visible = !*visible);
    }

    pub(super) fn details_view(&self) -> AnyView {
        AnyView::new(view! { box() })
    }
}
