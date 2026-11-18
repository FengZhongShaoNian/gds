mod fonts;
mod grid;
mod ime;
mod palette;
#[cfg(test)]
mod tests;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::vocab::{Key, Modifiers};
use zgui::{component, view};

use super::Terminal;

/// Focusable terminal viewport component hosting a `CustomElement` grid.
#[component]
pub fn TerminalView(
    terminal: Terminal,
    #[prop(optional)] on_key: Option<UnsyncCallback<(Key, Modifiers), bool>>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
) -> impl IntoView {
    let _ = (terminal, on_key);
    crate::ui::styles::install_ui_stylesheets();
    view! {
        box(
            class = "gds-terminal-view",
            node_ref = node_ref.unwrap_or_default(),
            tabindex = Focus::Sequential,
            class = class
        ) {
            {grid::TerminalGrid::element()}
        }
    }
}
