use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn NavItemLabel(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        text(class = "gds-nav-item__label", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
