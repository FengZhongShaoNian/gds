//! Composable collapsible sidebar shell and section layout parts.

mod content;
mod footer;
mod header;
pub mod presentation;
mod section;
#[cfg(test)]
mod tests;

pub use content::{SideBarContent, SideBarContentProps};
pub use footer::{SideBarFooter, SideBarFooterProps};
pub use header::{SideBarHeader, SideBarHeaderProps};
pub use presentation::SidebarPresentation;
pub use section::{
    SideBarSection, SideBarSectionContent, SideBarSectionContentProps, SideBarSectionHeader,
    SideBarSectionHeaderProps, SideBarSectionProps, SideBarSectionTitle, SideBarSectionTitleProps,
};

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, css, view};

const SIDEBAR_SHEET: &str = "gds-ui-sidebar";

const SIDEBAR_CSS: &str = css!(
    ".gds-sidebar {
        display: flex;
        flex-direction: column;
        flex: 0 0 auto;
        height: 100%;
        width: var(--gds-sidebar-width, 222px);
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
        transition: width var(--gds-motion-normal, 160ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }
    .gds-sidebar[data-collapsed=\"true\"] {
        width: var(--gds-collapsed-sidebar-width, 58px);
    }
    .gds-sidebar__header,
    .gds-sidebar__footer {
        display: flex;
        flex-direction: column;
        flex-shrink: 0;
        width: 100%;
        min-width: 0px;
    }
    .gds-sidebar__content {
        display: block;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow-y: auto;
        overflow-x: hidden;
    }
    .gds-sidebar__content-inner {
        display: flex;
        flex-direction: column;
        width: 100%;
        min-width: 0px;
        min-height: 100%;
    }
    .gds-sidebar__section {
        display: flex;
        flex-direction: column;
        flex-shrink: 0;
        width: 100%;
        min-width: 0px;
        padding: 0px var(--gds-sidebar-padding, 10px);
    }
    .gds-sidebar__section-header {
        display: flex;
        flex-direction: row;
        align-items: center;
        flex-shrink: 0;
        width: 100%;
        min-width: 0px;
        min-height: 34px;
        gap: 4px;
    }
    .gds-sidebar__section-title {
        flex: 1 1 0%;
        min-width: 0px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        padding: 0px var(--gds-sidebar-section-padding, 6px);
        font-size: var(--gds-font-size-xs, 12px);
        font-weight: 500;
        color: var(--gds-muted-foreground);
        transition: opacity var(--gds-motion-normal, 160ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }
    .gds-sidebar[data-collapsed=\"true\"] .gds-sidebar__section-title {
        opacity: 0;
    }
    .gds-sidebar__section-content {
        display: flex;
        flex-direction: column;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        gap: 2px;
        overflow-y: auto;
        overflow-x: hidden;
    }"
);

pub(super) fn install_sidebar_stylesheets() {
    crate::ui::styles::install_ui_stylesheets();
    zgui::view::install_stylesheet(SIDEBAR_SHEET, SIDEBAR_CSS);
}

/// Outer collapsible sidebar column styled with `--gds-sidebar-bg`.
#[component]
pub fn SideBar(
    #[prop(into, default = Signal::stored_local(false))] collapsed: Signal<bool, LocalStorage>,
    #[prop(into, optional)] label: Option<String>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    install_sidebar_stylesheets();
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
