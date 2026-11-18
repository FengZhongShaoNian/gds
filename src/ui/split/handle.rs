use zgui::prelude::*;
use zgui::{component, view};

use super::State;

/// Interactive divider handle for [`super::VerticalSplit`].
#[component]
pub fn SplitHandle(state: State, #[prop(into, optional)] class: Classes) -> impl IntoView {
    let _ = state;
    view! {
        box(class = "gds-split__handle", class = class, tabindex = Focus::Sequential)
    }
}
