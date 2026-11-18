//! Shared timestamp presentation and reactive application clock for GDS.

mod clock;
mod presentation;
#[cfg(test)]
mod tests;
mod timestamp;

use std::{cell::RefCell, time::SystemTime};

use serde::{Deserialize, Serialize};
use zgui::reactive::{LocalStorage, RwSignal, prelude::*};

use crate::app::Gds;
pub use clock::{Clock, ClockProps};
pub use presentation::{Presentation, precise};
pub use timestamp::{Timestamp, TimestampView, TimestampViewProps};

/// How shared timestamp elements present a value.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, crate::ConfigEditor,
)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// Exact local date and time, e.g. `2026-10-25 01:54`.
    #[setting(label = "form-time-absolute")]
    Absolute,
    /// Localized distance from the current time, e.g. `3 minutes ago`.
    #[default]
    #[setting(label = "form-time-relative")]
    Relative,
}

thread_local! {
    static SHARED_FORMAT: RefCell<Option<RwSignal<Format, LocalStorage>>> = const { RefCell::new(None) };
    static SHARED_TICK: RefCell<Option<RwSignal<u64, LocalStorage>>> = const { RefCell::new(None) };
}

fn ensure_format_signal() -> RwSignal<Format, LocalStorage> {
    SHARED_FORMAT.with(|slot| {
        if let Some(sig) = *slot.borrow() {
            return sig;
        }
        let sig = if let Some(gds) = Gds::try_current() {
            gds.with(|| RwSignal::new_local(Format::default()))
        } else {
            RwSignal::new_local(Format::default())
        };
        *slot.borrow_mut() = Some(sig);
        sig
    })
}

pub(crate) fn tick_signal() -> RwSignal<u64, LocalStorage> {
    SHARED_TICK.with(|slot| {
        if let Some(sig) = *slot.borrow() {
            return sig;
        }
        let sig = if let Some(gds) = Gds::try_current() {
            gds.with(|| RwSignal::new_local(0_u64))
        } else {
            RwSignal::new_local(0_u64)
        };
        *slot.borrow_mut() = Some(sig);
        sig
    })
}

/// Install or reconfigure the shared timestamp format.
pub fn apply(format: Format) {
    ensure_format_signal().set(format);
}

/// Read the active shared timestamp format.
#[must_use]
pub fn format() -> Format {
    ensure_format_signal().get()
}

/// Current wall-clock timestamp used when classifying relative times.
#[must_use]
pub fn now() -> SystemTime {
    SystemTime::now()
}
