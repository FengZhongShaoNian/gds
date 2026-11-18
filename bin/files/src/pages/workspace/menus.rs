#![allow(dead_code)]
//! Application and selection menus, and the persisted column-layout editor.

mod pages;

use std::path::PathBuf;

use gds::ui::menu::MenuEntry;
use zgui::prelude::*;
use zgui::view;

use super::{
    model::{Row, RowKey},
    ui::Browser,
};
use crate::{
    entities::{
        file::columns::Column,
        location::{Location, volumes},
    },
    shared::lib::selection::Selection,
};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct DirectoryTarget {
    tab: u64,
    path: PathBuf,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct SelectionTarget {
    tab: u64,
    location: Location,
    row: RowKey,
    selected: Selection<RowKey>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Surface {
    Closed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ColumnScope {
    Location,
    Default,
}

#[derive(Clone)]
pub(super) struct Menus {
    pub surface: RwSignal<Surface, LocalStorage>,
}

impl Menus {
    pub(super) fn new() -> Self {
        Self {
            surface: RwSignal::new_local(Surface::Closed),
        }
    }

    pub(super) fn is_open(&self) -> bool {
        !matches!(self.surface.get_untracked(), Surface::Closed)
    }

    pub(super) fn close(&self) {
        self.surface.set(Surface::Closed);
    }
}

impl Browser {
    pub(super) fn menu_key(&self, _event: &zgui::vocab::KeyEvent) -> bool {
        false
    }

    pub(super) fn app_menu(&self) -> AnyView {
        AnyView::new(view! { box() })
    }

    pub(super) fn show_app_menu(&self) {}

    pub(super) fn page_menu(&self) -> AnyView {
        let _: Option<Vec<MenuEntry>> = self.special_page_entries();
        AnyView::new(view! { box() })
    }

    pub(super) fn show_context_menu(&self) {}

    pub(super) fn row_menu(&self, view: AnyView, _index: usize) -> AnyView {
        view
    }

    pub(super) fn sidebar_volume_menu(&self, view: AnyView, _volume: &volumes::Volume) -> AnyView {
        view
    }

    pub(super) fn directory_menu(&self, view: AnyView) -> AnyView {
        view
    }

    pub(super) fn tab_menu(&self, view: AnyView, _id: u64) -> AnyView {
        view
    }

    pub(super) fn place_location(_row: &Row) -> Option<Location> {
        None
    }

    pub(super) fn open_selected_tab(&self) {}

    pub(super) fn copy_selected_paths(&self) {}

    pub(super) fn change_volume(&self, _action: volumes::DeviceAction) {}

    pub(super) fn show_help(&self) {
        self.open_page(super::model::Pane::Help);
    }

    pub(super) fn show_about(&self) {
        self.0.prompt.set(super::model::Prompt::About);
    }

    pub(super) fn column_menu(&self) -> AnyView {
        AnyView::new(view! { box() })
    }

    pub(super) fn show_sort_menu(&self) {}

    pub(super) fn configure_columns(&self) {}

    pub(super) fn sort_by(&self, _column: Column) {}

    pub(super) fn menu_overlay(&self) -> AnyView {
        AnyView::new(view! { box() })
    }
}
