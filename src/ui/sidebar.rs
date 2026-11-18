//! Composable collapsible sidebar shell and section layout parts.

mod content;
mod footer;
mod header;
pub mod presentation;
mod section;
#[cfg(test)]
mod tests;

pub use content::SideBarContent;
pub use footer::SideBarFooter;
pub use header::SideBarHeader;
pub use presentation::SidebarPresentation;
pub use section::{
    SideBarSection, SideBarSectionContent, SideBarSectionHeader, SideBarSectionTitle,
};

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

/// Outer collapsible sidebar column styled with `--gds-sidebar-bg`.
#[component]
pub fn SideBar(
    #[prop(into, default = Signal::stored_local(false))] collapsed: Signal<bool, LocalStorage>,
    #[prop(into, optional)] label: Option<String>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    let label = label.unwrap_or_else(|| "Sidebar".to_owned());
    view! {
        column(
            class = "gds-sidebar",
            attr:data-collapsed = move || collapsed.get().then(|| "true".to_owned()),
            a11y:role = Role::Complementary,
            a11y:label = label,
            class = class,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
