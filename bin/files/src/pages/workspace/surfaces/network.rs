#![allow(dead_code)]

use zgui::prelude::*;
use zgui::view;

use crate::pages::workspace::ui::Browser;

impl Browser {
    pub(in crate::pages::workspace) fn network_content(&self) -> AnyView {
        AnyView::new(view! { box() })
    }
}
