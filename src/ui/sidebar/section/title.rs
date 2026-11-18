use zgui::prelude::*;
use zgui::{component, view};

use super::super::SidebarContext;

#[component]
pub fn SideBarSectionTitle(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    super::super::install_sidebar_stylesheets();
    let pres = SidebarContext::current().map(|ctx| ctx.presentation);
    view! {
        text(
            class = "gds-sidebar__section-title",
            style:opacity = move || pres.map(|p| format!("{:.3}", p.get().label_opacity())),
            style:pointer-events = move || {
                pres.and_then(|p| (!p.get().actions_visible()).then(|| "none".to_owned()))
            },
            class = class,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
