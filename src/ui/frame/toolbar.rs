use zgui::prelude::*;
use zgui::{component, view};

/// Fixed-height window top toolbar.
#[component]
pub fn Toolbar(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        row(class = "gds-toolbar", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
