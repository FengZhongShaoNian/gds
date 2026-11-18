use zgui::prelude::*;
use zgui::{component, view};

use super::NavItemContext;

#[component]
pub fn NavItemActions(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    super::install_navigation_stylesheets();
    let pres = NavItemContext::current().map(|ctx| ctx.presentation);
    let no_drag = zgui::prelude::try_use_window().map(|w| w.no_drag_handler());
    view! {
        row(
            class = "gds-nav-item__actions",
            style:opacity = move || pres.map(|p| format!("{:.3}", p.get().actions_opacity())),
            style:pointer-events = move || {
                pres.map(|p| {
                    if p.get().actions_visible() {
                        "auto".to_owned()
                    } else {
                        "none".to_owned()
                    }
                })
            },
            style:visibility = move || {
                pres.map(|p| {
                    if p.get().actions_visible() {
                        "visible".to_owned()
                    } else {
                        "hidden".to_owned()
                    }
                })
            },
            class = class,
            on:pointer_down = move |ev: &mut zgui::view::EventCx<'_, zgui::view::events::PointerDown>| {
                ev.stop_propagation();
                if let Some(ref handler) = no_drag {
                    handler(ev);
                }
            },
            on:click = |ev: &mut zgui::view::EventCx<'_, zgui::view::events::Click>| {
                ev.stop_propagation();
            },
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
