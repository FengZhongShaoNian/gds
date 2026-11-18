//! A vertical split whose preferred height survives temporary space constraints.

mod handle;
mod state;
#[cfg(test)]
mod tests;

pub use handle::{SplitHandle, SplitHandleProps};
pub use state::{Options, State, constrained_height};

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

pub const DIVIDER_HEIGHT: f32 = 9.;

/// Vertical split pane hosting leading/trailing views separated by a draggable handle.
#[component]
pub fn VerticalSplit(
    state: State,
    #[prop(optional)] on_commit: Option<UnsyncCallback<f32>>,
    #[prop(into, optional)] label: Option<String>,
    leading: ChildrenFn,
    trailing: ChildrenFn,
) -> impl IntoView {
    let _ = (label, on_commit);
    crate::ui::styles::install_ui_stylesheets();
    let collapsed = state.collapsed();
    let width = state.width();
    view! {
        column(class = "gds-split", attr:data-collapsed = move || collapsed.get().then(|| "true".to_owned())) {
            box(class = "gds-split__leading") {{leading.view()}}
            SplitHandle(state = state)
            box(
                class = "gds-split__trailing",
                var:--gds-split-size = move || Some(format!("{:.1}px", width.get()))
            ) {{trailing.view()}}
        }
    }
}

/// Optional change payload emitted when split dragging commits a height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Changed(pub f32);
