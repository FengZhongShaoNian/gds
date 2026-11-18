use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn NavItemHeader(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    super::install_navigation_stylesheets();
    view! {
        row(class = "gds-nav-item__header", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
