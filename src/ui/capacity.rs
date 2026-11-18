//! Shared capacity meter: the filled part is occupied or reserved; the
//! remaining track is available. Callers supply a localized byte summary.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

pub const EXTRA_ROW_HEIGHT: f32 = 32.;

/// Displays a capacity summary label alongside a filled progress bar.
#[component]
pub fn CapacityBar(
    #[prop(into)] label: Signal<String, LocalStorage>,
    #[prop(into, default = Signal::stored_local(None))] fraction: Signal<Option<f32>, LocalStorage>,
    #[prop(default = false)] compact: bool,
    #[prop(into, default = Signal::stored_local(false))] unavailable: Signal<bool, LocalStorage>,
    #[prop(into, default = Signal::stored_local(false))] selected: Signal<bool, LocalStorage>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        column(
            class = "gds-capacity",
            attr:data-compact = compact.then(|| "true".to_owned()),
            attr:data-unavailable = move || unavailable.get().then(|| "true".to_owned()),
            attr:data-selected = move || selected.get().then(|| "true".to_owned()),
            class = class,
            {..attrs}
        ) {
            text(class = "gds-capacity__label") {{move || label.get()}}
            box(class = "gds-capacity__track") {
                box(
                    class = "gds-capacity__fill",
                    var:--gds-capacity-width = move || {
                        let pct = fraction.get().map_or(0.0, |v| v.clamp(0.0, 1.0) * 100.0);
                        Some(format!("{pct:.2}%"))
                    }
                )
            }
        }
    }
}
