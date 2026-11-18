mod actions;
mod title;

pub use actions::PageActions;
pub use title::PageTitle;

use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn PageHeader(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        row(class = "gds-page-header", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
