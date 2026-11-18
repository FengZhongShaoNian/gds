//! Independent diagnostics window for application logs and registered tasks.

mod filters;
mod navigation;
#[cfg(test)]
mod tests;
mod window;

pub use window::{OpenLogs, Options, init, is_enabled, open, store};

use zgui::prelude::*;
use zgui::{component, view};

/// Log viewer component rendered inside a secondary diagnostics window.
#[component]
pub fn LogViewer() -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        column(class = "gds-log-viewer")
    }
}
