use zgui::app::{Fonts, LineRequest, ResolvedLineMetrics};
use zgui::view::Ident;

/// Resolve terminal monospace line metrics via `zgui::app::Fonts::shaper`.
#[allow(dead_code)] // Seed skeleton for Wave 2 ui-terminal-tools.
#[must_use]
pub(super) fn cell_metrics(
    fonts: &Fonts,
    family: &str,
    size_device_px: f32,
) -> Option<ResolvedLineMetrics> {
    let families = [Ident::new(family)];
    fonts.shaper().line_metrics(&LineRequest {
        families: &families,
        weight: 400,
        italic: false,
        size_device_px,
        letter_spacing: 0.0,
        ligatures: false,
    })
}
