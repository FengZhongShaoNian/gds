//! Suite settings presentation: rows, numeric ranges, and keyboard-accessible sliders.

pub mod form;
mod row;

pub use row::*;

use crate::appearance::typography;
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui::SliderProps;
use zgui_ui_primitives::Binding;

pub const CONTROL_WIDTH: f32 = 260.0;
pub(crate) const RESET_WIDTH: f32 = 32.0;
pub(crate) const CONTROL_GAP: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberRange {
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

impl NumberRange {
    #[must_use]
    pub fn quantize(self, value: f32) -> f32 {
        if !value.is_finite() {
            return self.max;
        }
        let value = value.clamp(self.min, self.max);
        (((value - self.min) / self.step).round() * self.step + self.min).clamp(self.min, self.max)
    }

    #[must_use]
    pub fn equivalent(self, left: f32, right: f32) -> bool {
        left.is_finite() && right.is_finite() && self.quantize(left) == self.quantize(right)
    }
}

pub const PERCENTAGE_RANGE: NumberRange = NumberRange {
    min: 0.0,
    max: 100.0,
    step: 1.0,
};

pub const FONT_SIZE_RANGE: NumberRange = NumberRange {
    min: typography::MIN_SIZE,
    max: typography::MAX_SIZE,
    step: 1.0,
};

/// Keyboard-accessible quantized numeric slider paired with a live readout.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn NumberControl(
    #[prop(into, optional)] value: Binding<f64>,
    range: NumberRange,
    #[prop(optional)] on_change: Option<UnsyncCallback<f64>>,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
    #[prop(into, optional)] label: Option<String>,
    #[prop(optional)] labelled_by: Option<NodeRef>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        box(class = "gds-setting-number", class = class, {..attrs}) {
            Slider(
                value = value,
                min = f64::from(range.min),
                max = f64::from(range.max),
                step = f64::from(range.step),
                disabled = disabled,
                label = label,
                labelled_by = labelled_by,
                node_ref = node_ref,
                on_change = on_change
            )
        }
    }
}
