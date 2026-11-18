use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn NavItemActions(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        row(class = "gds-nav-item__actions", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
