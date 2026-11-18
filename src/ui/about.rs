//! Shared About content, independent of its dialog, page, or window host.

mod metadata;
#[cfg(test)]
mod tests;

pub use metadata::{APPLICATIONS, Application, Credit};

use zgui::prelude::*;
use zgui::{component, view};

pub const WIDTH: f32 = 480.;
pub const HEIGHT: f32 = 360.;

/// Shared application About view displaying version, authors, credits, and license.
#[component]
pub fn About(application: &'static Application) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        column(class = "gds-about", attr:data-binary = application.binary) {
            text(class = "gds-about__name") {{application.name}}
            text(class = "gds-about__version") {{format!("{} ({})", application.version, application.build_tag)}}
        }
    }
}
