//! A vertical split whose preferred height survives temporary space constraints.

mod handle;
mod state;
#[cfg(test)]
mod tests;

pub use handle::{SplitHandle, SplitHandleProps};
pub use state::{Options, State, constrained_height};

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, css, view};

pub const DIVIDER_HEIGHT: f32 = 9.;
pub const KEY_STEP: f32 = 12.;

const SPLIT_SHEET: &str = "gds-ui-split";

const SPLIT_CSS: &str = css!(
    ".gds-split {
        display: flex;
        flex-direction: column;
        width: 100%;
        height: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }
    .gds-split__leading {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }
    .gds-split__trailing {
        display: flex;
        flex-direction: column;
        flex: 0 0 auto;
        height: var(--gds-split-size, 220px);
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }
    .gds-split[data-collapsed=\"true\"] .gds-split__trailing {
        height: 0px;
    }
    .gds-split__handle-slot {
        position: relative;
        z-index: 10;
        width: 100%;
        height: 1px;
        flex-shrink: 0;
    }
    .gds-split__handle {
        position: absolute;
        left: 0px;
        right: 0px;
        top: -4px;
        height: 9px;
        display: flex;
        align-items: center;
        justify-content: center;
        background-color: transparent;
        cursor: ns-resize;
    }
    .gds-split__line {
        position: absolute;
        left: 0px;
        right: 0px;
        top: 4px;
        height: 1px;
        background-color: var(--gds-border);
        transition: background-color var(--gds-motion-fast, 100ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }
    .gds-split__handle:hover .gds-split__line,
    .gds-split__handle[data-dragging=\"true\"] .gds-split__line {
        background-color: var(--gds-primary);
    }
    .gds-split__handle:focus-visible {
        outline: none;
    }
    .gds-split__handle:focus-visible .gds-split__line {
        outline: 1px solid var(--gds-primary);
        outline-offset: 1px;
    }"
);

pub(super) fn install_split_stylesheets() {
    crate::ui::styles::install_ui_stylesheets();
    zgui::view::install_stylesheet(SPLIT_SHEET, SPLIT_CSS);
}

#[derive(Clone)]
pub(super) struct SplitContext {
    pub(super) container: NodeRef,
    pub(super) label: Option<String>,
    pub(super) on_commit: Option<UnsyncCallback<f32>>,
}

/// Vertical split pane hosting leading/trailing views separated by a draggable handle.
#[component]
pub fn VerticalSplit(
    state: State,
    #[prop(optional)] on_commit: Option<UnsyncCallback<f32>>,
    #[prop(into, optional)] label: Option<String>,
    leading: ChildrenFn,
    trailing: ChildrenFn,
) -> impl IntoView {
    install_split_stylesheets();
    let container = NodeRef::new();
    provide_local_context(SplitContext {
        container,
        label,
        on_commit,
    });
    let collapsed = state.collapsed();
    let width = state.width();
    view! {
        column(
            node_ref = container,
            class = "gds-split",
            attr:data-collapsed = move || collapsed.get().then(|| "true".to_owned())
        ) {
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
