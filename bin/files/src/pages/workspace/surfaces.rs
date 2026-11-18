#![allow(dead_code)]
//! Page-specific table metadata uses each domain's own values and service state.

mod network;

use gds::ui::IconName;
use zgui::prelude::*;
use zgui::view;
use zgui_ui_primitives::Binding;

use super::{model::Row, ui::Browser};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct CollectionBodyState {
    pub icon: Option<IconName>,
    pub title: String,
    pub detail: Option<String>,
    pub retry: bool,
    pub loading: bool,
    pub reconnect: bool,
}

impl CollectionBodyState {
    pub(super) fn view(self, _browser: &Browser) -> AnyView {
        AnyView::new(view! {
            column {
                text {{self.title}}
            }
        })
    }
}

impl Browser {
    pub(super) fn collection_notice(&self) -> Option<AnyView> {
        None
    }

    pub(super) fn collection_body_state(&self) -> Option<CollectionBodyState> {
        None
    }

    pub(super) fn collection_empty_label(&self) -> Option<&'static str> {
        None
    }

    pub(super) fn collection_columns(&self) -> Option<Vec<(&'static str, f32)>> {
        None
    }

    pub(super) fn collection_header(&self) -> Option<AnyView> {
        None
    }

    pub(super) fn collection_cells(
        &self,
        _row: &Row,
        _selected: Binding<bool>,
    ) -> Option<Vec<AnyView>> {
        None
    }
}
