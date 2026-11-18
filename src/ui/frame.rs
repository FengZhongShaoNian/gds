//! Shared GDS outer CSD window frame, toolbar, footer, drag strip, and workspace wrapper.

mod drag;
mod footer;
mod toolbar;
mod workspace;

pub use drag::WindowDragArea;
pub use footer::Footer;
pub use toolbar::Toolbar;
pub use workspace::WorkspaceFrame;

use zgui::prelude::*;
use zgui::{component, view};

use crate::app::Gds;

/// Outer window shell installing `"gds-appearance"` + `"gds-ui-base"` and framing client chrome.
#[component]
pub fn Frame(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    if let Some(gds) = Gds::try_current() {
        let _ = gds.appearance().install_window_sheet();
    }
    crate::ui::styles::install_ui_stylesheets();
    view! {
        column(class = "gds-frame", class = class, {..attrs}) {
            box(class = "gds-frame-content") {
                {children.into_view_once()}
            }
        }
    }
}
