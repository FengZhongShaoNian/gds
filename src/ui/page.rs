//! Shared content page layout containers and history stack.

mod content;
mod header;
mod notice;
pub mod stack;
mod table;

pub use content::PageContent;
pub use header::{PageActions, PageHeader, PageTitle};
pub use notice::PageNotice;
pub use table::PageTableArea;

use zgui::prelude::*;
use zgui::{component, view};

/// Top-level workspace content page container.
#[component]
pub fn ContentPage(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        column(class = "gds-content-page", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
