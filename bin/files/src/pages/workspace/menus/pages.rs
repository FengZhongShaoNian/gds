#![allow(dead_code)]

use gds::ui::menu::MenuEntry;
use zgui::prelude::*;

use crate::pages::workspace::ui::Browser;

impl Browser {
    pub(super) fn special_page_entries(&self) -> Option<Vec<MenuEntry>> {
        None
    }

    pub(in crate::pages::workspace) fn special_page_actions(&self) -> Option<AnyView> {
        None
    }
}
