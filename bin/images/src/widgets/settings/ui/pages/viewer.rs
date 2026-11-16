//! Viewer settings section (smooth scaling removed cleanly along with `gpui-fast`).

use gds_uim::el;
use gpui_kit::{App, Entity, IntoElement, div, prelude::*};

use super::super::{
    View,
    controls::{Field, section_heading, toggle_row},
};
use crate::shared::i18n::text;

pub(super) fn render(state: &View, view: &Entity<View>, cx: &App) -> impl IntoElement {
    let prefs = &state.preferences;
    let checkerboard = toggle_row(
        view,
        Field::CheckerboardBackdrop,
        prefs.checkerboard_backdrop,
        false,
        prefs,
        cx,
    );
    el! {
        .div {
            flex; flex_col; w_full; gap_6;
        } [
            $(section_heading(text("settings-viewer", cx), cx)),
            .div { flex; flex_col; w_full; } [
                checkerboard,
            ],
        ]
    }
}
