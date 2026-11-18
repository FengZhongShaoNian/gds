use zgui::prelude::*;
use zgui::{component, view};

/// Titlebar region initiating compositor window move/maximize on primary press.
#[component]
pub fn WindowDragArea(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    let handler = zgui::prelude::try_use_window().map(|w| w.move_drag_handler());
    view! {
        row(
            class = "gds-window-drag-area",
            class = class,
            on:pointer_down = move |ev: &mut zgui::view::EventCx<'_, zgui::view::events::PointerDown>| {
                if let Some(ref handler) = handler {
                    handler(ev);
                }
            },
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
