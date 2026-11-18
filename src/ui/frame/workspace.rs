use zgui::prelude::*;
use zgui::{component, view};

/// Flexible workspace body region inside [`super::Frame`].
#[component]
pub fn WorkspaceFrame(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    super::install_frame_stylesheets();
    view! {
        row(class = "gds-workspace-frame", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
