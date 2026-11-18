use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn PageTableArea(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    super::install_page_stylesheets();
    view! {
        box(class = "gds-page-table-area", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
