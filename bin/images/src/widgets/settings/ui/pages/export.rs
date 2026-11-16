//! Export and privacy settings section.

use gds_uim::el;
use gpui_kit::{App, Entity, IntoElement, div, prelude::*};

use super::super::{
    View,
    controls::{Field, format_quality, section_heading, toggle_row},
};
use crate::shared::i18n::text;

pub(super) fn render(state: &View, view: &Entity<View>, cx: &App) -> impl IntoElement {
    let prefs = &state.preferences;
    let strip_row = toggle_row(
        view,
        Field::StripPrivacyOnCopy,
        prefs.strip_privacy_on_copy,
        false,
        prefs,
        cx,
    );
    let quality_row = state.jpeg_quality.row(
        Field::DefaultJpegQuality,
        format_quality(prefs.default_jpeg_quality),
        prefs,
        view,
        cx,
    );
    el! {
        .div {
            flex; flex_col; w_full; gap_6;
        } [
            $(section_heading(text("settings-export", cx), cx)),
            .div { flex; flex_col; w_full; } [
                strip_row,
                quality_row,
            ],
        ]
    }
}
