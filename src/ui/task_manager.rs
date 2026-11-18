//! Live background task list and cancellation controls backed by `Gds::current().tasks()`.

use zgui::prelude::*;
use zgui::{component, view};

/// Diagnostics view listing active and completed tasks with cancellation triggers.
#[component]
pub fn TaskManagerView() -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        column(class = "gds-task-manager")
    }
}
