mod content;
mod header;
mod title;

pub use content::SideBarSectionContent;
pub use header::SideBarSectionHeader;
pub use title::SideBarSectionTitle;

use zgui::prelude::*;
use zgui::{component, view};

#[component]
pub fn SideBarSection(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        column(class = "gds-sidebar__section", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
