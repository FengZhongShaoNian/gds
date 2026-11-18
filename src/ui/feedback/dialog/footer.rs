use zgui::prelude::*;
use zgui::{component, view};

/// Footer band aligned to the trailing edge of a GDS modal dialog.
#[component]
pub fn DialogFooter(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        row(class = "gds-dialog-footer", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
