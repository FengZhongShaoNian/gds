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
pub use presentation::{
    SidebarPresentation, SidebarState, use_sidebar_presentation, use_sidebar_presentation_widths,
};
pub use section::{
    SideBarSection, SideBarSectionContent, SideBarSectionContentProps, SideBarSectionHeader,
    SideBarSectionHeaderProps, SideBarSectionProps, SideBarSectionTitle, SideBarSectionTitleProps,
};

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, provide_local_context, use_local_context};
use zgui::{component, css, view};

use crate::appearance::tokens;

#[derive(Clone, Copy)]
pub(crate) struct SidebarContext {
    pub(crate) presentation: Signal<SidebarPresentation, LocalStorage>,
}

impl SidebarContext {
    pub(crate) fn current() -> Option<Self> {
        zgui::reactive::is_ui_thread()
            .then(use_local_context::<Self>)
            .flatten()
    }
}

const SIDEBAR_SHEET: &str = "gds-ui-sidebar";

const SIDEBAR_CSS: &str = css!(
    ".gds-sidebar {
        display: flex;
        flex-direction: column;
        flex: 0 0 auto;
        height: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
        color: var(--gds-sidebar-foreground);
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
    .gds-sidebar__content--static {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
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
        font-weight: 400;
        color: var(--gds-muted-foreground);
    }
    .gds-sidebar__section-content {
        display: flex;
        flex-direction: column;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        gap: 2px;
    }
    .gds-sidebar__section-content--scrollable {
        display: block;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow-y: auto;
        overflow-x: hidden;
    }
    .gds-sidebar__section-content-inner {
        display: flex;
        flex-direction: column;
        width: 100%;
        min-width: 0px;
        gap: 2px;
    }"
);

pub(super) fn install_sidebar_stylesheets() {
    crate::ui::styles::install_ui_stylesheets();
    zgui::view::install_stylesheet(SIDEBAR_SHEET, SIDEBAR_CSS);
}

/// Composable vertical navigation column; the enclosing [`crate::ui::Frame`] paints the single
/// sidebar surface material and vertical hairline divider.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn SideBar(
    #[prop(into, optional)] state: Option<Signal<SidebarState, LocalStorage>>,
    #[prop(into, optional)] presentation: Option<Signal<SidebarPresentation, LocalStorage>>,
    #[prop(into, optional)] collapsed: Option<Signal<bool, LocalStorage>>,
    #[prop(default = tokens::SIDEBAR_WIDTH)] expanded_width: f32,
    #[prop(default = tokens::COLLAPSED_SIDEBAR_WIDTH)] collapsed_width: f32,
    #[prop(into, optional)] label: Option<String>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    install_sidebar_stylesheets();

    let effective_presentation = if let Some(pres) = presentation {
        pres
    } else if let Some(state_sig) = state {
        use_sidebar_presentation_widths(state_sig, expanded_width, collapsed_width)
    } else if let Some(collapsed_sig) = collapsed {
        let derived_state =
            Signal::derive_local(move || SidebarState::from_collapsed(collapsed_sig.get()));
        use_sidebar_presentation_widths(derived_state, expanded_width, collapsed_width)
    } else {
        Signal::stored_local(SidebarPresentation::from_width(
            expanded_width,
            expanded_width,
            collapsed_width,
        ))
    };

    provide_local_context(SidebarContext {
        presentation: effective_presentation,
    });

    let label = label.unwrap_or_else(|| "Sidebar".to_owned());
    view! {
        column(
            class = "gds-sidebar",
            style:width = move || Some(format!("{:.2}px", effective_presentation.get().width())),
            attr:data-collapsed = move || effective_presentation.get().collapsed().then(|| "true".to_owned()),
            a11y:role = Role::Complementary,
            a11y:label = label,
            class = class,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
