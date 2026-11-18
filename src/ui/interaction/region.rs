//! Keyboard focus regions and shared focus-ring styling helpers.

use zgui::prelude::*;
use zgui::{component, view};

/// Marks a region container with GDS focus-ring styling.
#[must_use]
pub fn surface(node_ref: NodeRef) -> Attrs {
    crate::ui::styles::install_ui_stylesheets();
    let _ = node_ref;
    Attrs::new().class_toggle(zgui::view::ClassName::new("gds-region-surface"), true)
}

/// Focusable landmark region applying `.gds-region-surface` styling.
#[component]
pub fn Region(
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        box(class = "gds-region-surface", node_ref = node_ref.unwrap_or_default(), class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
