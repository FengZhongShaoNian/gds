use zgui::prelude::*;
use zgui::vocab::PointerButton;
use zgui::{component, view};

/// Titlebar region initiating compositor window move/maximize on primary press.
#[component]
pub fn WindowDragArea(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    super::install_frame_stylesheets();
    let window = zgui::prelude::try_use_window();
    let move_handler = window.as_ref().map(WindowHandle::move_drag_handler);
    let menu_win = window;
    view! {
        row(
            class = "gds-window-drag-area",
            class = class,
            on:pointer_down = move |ev: &mut zgui::view::EventCx<'_, zgui::view::events::PointerDown>| {
                if ev.button == Some(PointerButton::Secondary) {
                    if let Some(ref w) = menu_win {
                        crate::ui::window::show_window_menu(w, ev.position);
                    }
                } else if let Some(ref handler) = move_handler {
                    handler(ev);
                }
            },
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
