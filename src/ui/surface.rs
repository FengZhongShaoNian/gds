//! Raised card container for panels and grouped settings blocks.

use zgui::prelude::*;
use zgui::{component, view};

/// Bordered raised card container styled by `--gds-panel`.
#[component]
pub fn CardSurface(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        box(class = "gds-card-surface", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
