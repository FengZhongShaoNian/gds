use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn PageContent(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        scroll(class = "gds-page-content", class = class, {..attrs}) {
            column {{children.into_view_once()}}
        }
    }
}
