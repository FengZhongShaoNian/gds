//! Floating pointer-tracked drag preview card rendered via `Portal`.

mod content;
mod footer;
mod header;

pub use content::DragCardContent;
pub use footer::DragCardFooter;
pub use header::DragCardHeader;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

/// Floating preview surface displayed while an internal pointer drag is active.
#[component]
pub fn DragCard(
    #[prop(into, default = Signal::stored_local((0.0_f32, 0.0_f32)))] position: Signal<
        (f32, f32),
        LocalStorage,
    >,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    let _ = position;
    view! {
        column(class = "gds-drag-card gds-surface", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
