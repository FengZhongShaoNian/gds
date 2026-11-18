//! Blank-space interaction container shared by content lists and tables.

#[cfg(test)]
mod tests;

use zgui::prelude::*;
use zgui::{component, view};

/// Visual density/alternating style for [`ListSurface`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListStyle {
    #[default]
    Striped,
    Plain,
}

/// Uniform list container applying GDS density styling.
#[component]
pub fn ListSurface(
    #[prop(default = ListStyle::Striped)] style: ListStyle,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    let _ = style;
    crate::ui::styles::install_ui_stylesheets();
    view! {
        column(
            class = "gds-list-surface",
            node_ref = node_ref.unwrap_or_default(),
            class = class,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
