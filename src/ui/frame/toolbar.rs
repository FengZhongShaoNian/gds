use zgui::prelude::*;
use zgui::{component, view};

/// Fixed-height window top toolbar.
#[component]
pub fn Toolbar(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    super::install_frame_stylesheets();
    view! {
        row(
            class = "gds-toolbar",
            style:height = format!("{}px", super::TOOLBAR_HEIGHT),
            class = class,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
