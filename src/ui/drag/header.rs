use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn DragCardHeader(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        row(class = "gds-drag-card__header", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
