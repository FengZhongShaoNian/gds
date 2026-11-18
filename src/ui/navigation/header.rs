use zgui::prelude::*;
use zgui::{component, view};

use super::NavItemContext;

#[component]
pub fn NavItemHeader(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    super::install_navigation_stylesheets();
    let ctx = NavItemContext::current();
    view! {
        row(
            class = "gds-nav-item__header",
            style:gap = move || {
                ctx.map(|c| format!("{:.2}px", c.presentation.get().interpolate(8.0, 0.0)))
            },
            style:padding-right = move || {
                ctx.filter(|c| c.trailing_reserve > 0.0).map(|c| {
                    format!(
                        "{:.2}px",
                        c.presentation.get().interpolate(c.trailing_reserve, 0.0)
                    )
                })
            },
            class = class,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
