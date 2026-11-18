use zgui::prelude::*;
use zgui::{component, view};

/// Fixed-height window footer status bar.
#[component]
pub fn Footer(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        row(class = "gds-footer", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
