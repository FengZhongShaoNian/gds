//! Reactive clock display synchronized across windows.

use zgui::prelude::*;
use zgui::{component, view};

use super::tick_signal;

/// Reactive clock component refreshing at minute ticks.
#[component]
pub fn Clock() -> impl IntoView {
    let ticks = tick_signal();
    view! {
        text(class = "gds-clock") {{move || {
            let _ = ticks.get();
            chrono::Local::now().format("%H:%M").to_string()
        }}}
    }
}
