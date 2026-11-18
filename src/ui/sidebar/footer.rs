use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn SideBarFooter(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        column(class = "gds-sidebar__footer", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
