use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn PageTableArea(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        box(class = "gds-page-table-area", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
