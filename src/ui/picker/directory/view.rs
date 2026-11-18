use zgui::prelude::*;
use zgui::{component, view};

use super::DirectoryPicker;

/// Directory chooser view component driven by [`DirectoryPicker`].
#[component]
pub fn DirectoryPickerView(
    picker: DirectoryPicker,
    #[prop(into, optional)] class: Classes,
) -> impl IntoView {
    let _ = picker;
    crate::ui::styles::install_ui_stylesheets();
    view! {
        column(class = "gds-directory-picker", class = class)
    }
}
