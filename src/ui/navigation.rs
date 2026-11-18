//! Navigation list items, pinned badges, and hover/corner action slots.

mod action;
mod actions;
mod content;
mod details;
mod header;
mod icon;
mod label;

pub use action::{
    CornerAction, CornerActionProps, CornerButton, CornerButtonProps, HoverAction,
    HoverActionProps, corner_action, corner_button, hover_action,
};
pub use actions::{NavItemActions, NavItemActionsProps};
pub use content::{NavItemContent, NavItemContentProps};
pub use details::{NavItemDetails, NavItemDetailsProps};
pub use header::{NavItemHeader, NavItemHeaderProps};
pub use icon::{PinnedIcon, PinnedIconProps, WithPin, WithPinProps, pinned_icon, with_pin};
pub use label::{NavItemLabel, NavItemLabelProps};

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

/// Interactive navigation row with selected and external/internal drop-target styling.
#[component]
pub fn NavItem(
    #[prop(into, default = Signal::stored_local(false))] selected: Signal<bool, LocalStorage>,
    #[prop(into, default = Signal::stored_local(false))] drop_target: Signal<bool, LocalStorage>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        row(
            class = "gds-nav-item",
            node_ref = node_ref.unwrap_or_default(),
            attr:data-selected = move || selected.get().then(|| "true".to_owned()),
            attr:data-drop-target = move || drop_target.get().then(|| "true".to_owned()),
            class = class,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
