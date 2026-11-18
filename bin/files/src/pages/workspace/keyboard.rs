#![allow(dead_code)]
//! Files command routing follows focus boundaries; native editors retain editing keys.

use zgui::prelude::*;
use zgui::view::EventCx;
use zgui::vocab::{KeyEvent, Modifiers};

use super::{model::Row, ui::Browser};
use crate::{
    entities::location::{LocationTarget, volumes},
    shared::ui::actions::Command,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SidebarRegion {
    Tabs,
    Locations,
    Volumes,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum SidebarRow {
    Tab(u64),
    Location(LocationTarget),
    Volume(volumes::VolumeId),
}

impl Browser {
    pub(super) fn dispatch(&self, _command: Command) {}

    pub(super) fn sidebar_node(&self, region: SidebarRegion) -> NodeRef {
        match region {
            SidebarRegion::Tabs => self.0.tabs_node,
            SidebarRegion::Locations => self.0.locations_node,
            SidebarRegion::Volumes => self.0.volumes_node,
        }
    }

    pub(super) fn sidebar_focus_changed(&self, region: SidebarRegion, focused: bool) {
        if focused {
            self.0.sidebar_region.set(Some(region));
        } else if self.0.sidebar_region.get_untracked() == Some(region) {
            self.0.sidebar_region.set(None);
        }
    }

    pub(super) fn sidebar_key(&self, _event: &KeyEvent, _region: SidebarRegion) {}

    pub(super) fn content_key(&self, _event: &KeyEvent) {}

    pub(super) fn search_key(&self, _event: &KeyEvent) {}

    pub(super) fn key_down(&self, event: &mut EventCx<'_, zgui::view::events::KeyDown>) {
        if let Some(command) = Command::from_key_event(event, event.modifiers)
            && !command.needs_content_focus()
        {
            self.dispatch(command);
            event.stop_propagation();
        }
    }

    pub(super) fn select_row(&self, index: usize, _modifiers: Modifiers) {
        self.update_selection(|state| {
            let tab = state.tab_mut();
            if let Some(key) = tab.rows.get(index).map(Row::key) {
                tab.navigation.view_mut().selection = std::iter::once(key.clone()).collect();
                tab.navigation.view_mut().anchor = Some(key.clone());
                tab.navigation.view_mut().cursor = Some(key);
            }
        });
    }

    pub(super) fn select_all(&self) {}
}
