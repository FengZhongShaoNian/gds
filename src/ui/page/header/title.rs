use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn PageTitle(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        text(class = "gds-page-title", a11y:role = Role::Heading, class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
