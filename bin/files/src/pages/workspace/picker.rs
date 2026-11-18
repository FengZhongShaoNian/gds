#![allow(dead_code)]
//! Native chooser controls over the existing lossless picker protocol.

use std::path::PathBuf;

use zgui::prelude::*;
use zgui::view;

use super::ui::Browser;

impl Browser {
    pub(super) fn initialize_picker(&self) {}

    pub(super) fn picker_footer(&self) -> AnyView {
        AnyView::new(view! { box() })
    }

    pub(super) fn activate_picker_path(&self, _path: PathBuf) {}

    pub(super) fn accept_picker(&self) {}

    pub(super) fn cancel_picker(&self) {
        self.update(|state| {
            if let Some(picker) = state.picker.as_mut() {
                picker.cancel();
            }
        });
    }

    pub(super) fn picker_key(&self, _event: &zgui::vocab::KeyEvent) -> bool {
        false
    }
}
