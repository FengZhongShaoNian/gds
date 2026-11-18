#![allow(dead_code)]
//! Files chrome keeps address navigation, folder filtering, and sidebar regions distinct.

use gds::ui::{frame::Toolbar, sidebar::SidebarPresentation};
use zgui::prelude::*;
use zgui::view;

use super::ui::Browser;

impl Browser {
    pub(super) fn menu_controls(&self) -> AnyView {
        AnyView::new(view! { row() })
    }

    pub(super) fn toolbar(&self) -> AnyView {
        let menu = self.menu_controls();
        let filter = self.folder_filter();
        AnyView::new(Toolbar(
            Classes::default(),
            Attrs::default(),
            Children::from_view(view! {
                row {
                    {menu}
                    {filter}
                }
            }),
        ))
    }

    pub(super) fn start_address_edit(&self) {}

    pub(super) fn finish_address_edit(&self) {}

    pub(super) fn folder_filter(&self) -> AnyView {
        AnyView::new(view! { row() })
    }

    pub(super) fn filter_available(&self) -> bool {
        false
    }

    pub(super) fn sidebar(&self, _presentation: SidebarPresentation) -> AnyView {
        AnyView::new(view! { column() })
    }
}
