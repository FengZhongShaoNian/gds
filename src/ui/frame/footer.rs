use zgui::prelude::*;
use zgui::{component, view};

/// Fixed-height window footer status bar.
#[component]
pub fn Footer(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    super::install_frame_stylesheets();
    view! {
        row(
            class = "gds-footer",
            style:height = format!("{}px", super::FOOTER_HEIGHT),
            class = class,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
