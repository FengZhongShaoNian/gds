use zgui::reactive::{LocalStorage, RwSignal, Signal, prelude::*};

use super::DIVIDER_HEIGHT;

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub min: f32,
    pub max: f32,
    pub initial: f32,
    pub collapse_below: Option<f32>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            min: 90.0,
            max: 600.0,
            initial: 220.0,
            collapse_below: None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct State {
    width: RwSignal<f32, LocalStorage>,
    collapsed: RwSignal<bool, LocalStorage>,
    options: Options,
}

impl State {
    #[must_use]
    pub fn new(options: Options) -> Self {
        Self {
            width: RwSignal::new_local(options.initial.clamp(options.min, options.max)),
            collapsed: RwSignal::new_local(false),
            options,
        }
    }

    #[must_use]
    pub fn width(self) -> Signal<f32, LocalStorage> {
        self.width.into()
    }

    pub fn set_width(self, width: f32) {
        self.width
            .set(width.clamp(self.options.min, self.options.max));
    }

    #[must_use]
    pub fn collapsed(self) -> Signal<bool, LocalStorage> {
        self.collapsed.into()
    }

    pub fn set_collapsed(self, collapsed: bool) {
        self.collapsed.set(collapsed);
    }
}

#[must_use]
pub fn constrained_height(
    preferred: Option<f32>,
    available: f32,
    top_min: f32,
    bottom_min: f32,
    default_bottom: f32,
) -> f32 {
    let usable = (available - DIVIDER_HEIGHT).max(0.);
    let top_min = top_min.min(usable / 2.);
    let maximum = (usable - bottom_min).max(top_min);
    preferred
        .filter(|value| value.is_finite())
        .unwrap_or(usable - default_bottom)
        .clamp(top_min, maximum)
}
