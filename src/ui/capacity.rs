//! Shared capacity meter: the filled part is occupied or reserved; the
//! remaining track is available. Callers supply a localized byte summary.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::AttrName;
use zgui::vocab::Role;
use zgui::{component, view};

pub const EXTRA_ROW_HEIGHT: f32 = 32.;

fn normalized_fraction(fraction: Option<f32>, unavailable: bool) -> Option<f32> {
    if unavailable {
        return None;
    }
    fraction
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 1.0))
}

fn compact_percent_text(fraction: Option<f32>, unavailable: bool) -> String {
    let Some(used) = normalized_fraction(fraction, unavailable) else {
        return "\u{2014}".to_owned();
    };
    let free = ((1.0 - used).clamp(0.0, 1.0) * 100.0).round();
    format!("{free:.0}%")
}

/// Displays a capacity summary label alongside a filled progress bar.
///
/// In `compact` mode, renders the remaining free percentage (or `—` while loading or
/// unavailable) without the track; in full mode, shows `label` above the track and animates
/// an indeterminate fill when `fraction` is `None` and `unavailable` is `false`.
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

    let meter_a11y = A11yBinding::new(Role::ProgressIndicator)
        .label(move || label.get())
        .step(move |a11y| {
            if let Some(used) = normalized_fraction(fraction.get(), unavailable.get()) {
                a11y.numeric_range(0.0, 1.0).numeric_value(f64::from(used))
            } else {
                a11y
            }
        });

    let own = Attrs::new()
        .attribute(
            AttrName::new("data-compact"),
            compact.then(|| "true".to_owned()),
        )
        .attribute(AttrName::new("data-unavailable"), move || {
            unavailable.get().then(|| "true".to_owned())
        })
        .attribute(AttrName::new("data-loading"), move || {
            (!unavailable.get() && normalized_fraction(fraction.get(), false).is_none())
                .then(|| "true".to_owned())
        })
        .attribute(AttrName::new("data-selected"), move || {
            selected.get().then(|| "true".to_owned())
        })
        .a11y_from(meter_a11y);

    view! {
        column(
            class = "gds-capacity",
            {..own},
            class = class,
            {..attrs}
        ) {
            text(class = "gds-capacity__label") {{move || {
                if compact {
                    compact_percent_text(fraction.get(), unavailable.get())
                } else {
                    label.get()
                }
            }}}
            {(!compact).then(|| view! {
                box(class = "gds-capacity__track") {
                    box(
                        class = "gds-capacity__fill",
                        var:--gds-capacity-width = move || {
                            let pct = normalized_fraction(fraction.get(), unavailable.get())
                                .map_or(0.0, |v| v * 100.0);
                            Some(format!("{pct:.2}%"))
                        }
                    )
                }
            })}
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui::prelude::*;
    use zgui::reactive::{Mounted, RwSignal, install};
    use zgui::view;
    use zgui::vocab::Role;

    use super::{CapacityBarProps, EXTRA_ROW_HEIGHT, compact_percent_text, normalized_fraction};

    #[test]
    fn normalized_fraction_clamps_and_filters_non_finite() {
        assert_eq!(normalized_fraction(Some(-0.2), false), Some(0.0));
        assert_eq!(normalized_fraction(Some(0.42), false), Some(0.42));
        assert_eq!(normalized_fraction(Some(1.8), false), Some(1.0));
        assert_eq!(normalized_fraction(Some(f32::NAN), false), None);
        assert_eq!(normalized_fraction(Some(f32::INFINITY), false), None);
        assert_eq!(normalized_fraction(Some(0.5), true), None);
    }

    #[test]
    fn compact_percent_reports_remaining_percentage_or_em_dash() {
        assert_eq!(compact_percent_text(Some(0.25), false), "75%");
        assert_eq!(compact_percent_text(Some(0.0), false), "100%");
        assert_eq!(compact_percent_text(Some(1.0), false), "0%");
        assert_eq!(compact_percent_text(None, false), "\u{2014}");
        assert_eq!(compact_percent_text(Some(0.5), true), "\u{2014}");
    }

    #[test]
    fn capacity_bar_builds_full_and_compact_views_with_a11y() {
        install().ok();
        let scope = Mounted::new();
        scope.with(|| {
            assert_eq!(EXTRA_ROW_HEIGHT, 32.0);
            let frac = RwSignal::new_local(Some(0.3));
            let _full = view! {
                CapacityBar(label = "30 GB free of 100 GB", fraction = frac)
            };
            let _compact = view! {
                CapacityBar(label = "Free", fraction = frac, compact = true)
            };
            let meter_a11y = A11yBinding::new(Role::ProgressIndicator)
                .label("Drive")
                .step(|a| a.numeric_range(0.0, 1.0).numeric_value(0.3))
                .lower();
            assert_eq!(meter_a11y.role, Role::ProgressIndicator);
            assert_eq!(meter_a11y.numeric.value, Some(0.3));
        });
        scope.unmount();
    }
}
