#![allow(dead_code)]
//! Terminal sessions belong to the window; the dock is only their current surface.

use std::path::PathBuf;

use gds::ui::sidebar::SidebarPresentation;
use zgui::prelude::*;
use zgui::view;

use super::ui::Browser;
use crate::entities::preferences::Preferences;

#[derive(Clone)]
pub(super) struct Dock {
    active: RwSignal<Option<u64>, LocalStorage>,
}

impl Dock {
    pub(super) fn new() -> Self {
        Self {
            active: RwSignal::new_local(None),
        }
    }
}

impl Browser {
    pub(super) fn apply_terminal_preferences(&self, _preferences: &Preferences) {}

    pub(super) fn toggle_terminal(&self) {
        self.0.terminal.update(|open| *open = !*open);
    }

    pub(super) fn new_terminal(&self) {}

    pub(super) fn new_terminal_at(&self, _path: PathBuf) {}

    pub(super) fn observe_terminals(&self) {}

    pub(super) fn terminal_workspace_resized(&self, _width: f32, _available: f32) {}

    pub(super) fn terminal_view(&self, _presentation: SidebarPresentation) -> AnyView {
        AnyView::new(view! { column() })
    }
}
