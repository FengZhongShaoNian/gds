#![allow(dead_code)]
//! The footer's task, selection, and editable clipboard summaries.

use zgui::prelude::*;
use zgui::view;

use super::ui::Browser;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Tasks,
    Selection,
    Clipboard,
}

impl Browser {
    pub(super) fn status_controls(&self) -> AnyView {
        AnyView::new(view! { row() })
    }
}
