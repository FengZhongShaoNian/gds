use zgui::prelude::*;
use zgui::{component, view};

use super::NavItemContext;

#[component]
pub fn NavItemDetails(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    super::install_navigation_stylesheets();
    let pres = NavItemContext::current().map(|ctx| ctx.presentation);
    view! {
        text(
            class = "gds-nav-item__details",
            style:opacity = move || pres.map(|p| format!("{:.3}", p.get().label_opacity())),
            class = class,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
