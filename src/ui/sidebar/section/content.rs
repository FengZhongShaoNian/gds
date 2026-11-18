use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn SideBarSectionContent(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        column(class = "gds-sidebar__section-content", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
