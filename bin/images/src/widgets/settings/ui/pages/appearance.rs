//! Appearance settings section mirroring suite theme and typography conventions.

use gds::appearance::theme::SourceKind;
use gds_uim::el;
use gpui_kit::{App, IntoElement, Window, component::ActiveTheme as _, div, prelude::*, px};

use super::super::{
    View,
    controls::{Field, format_percent, section_heading, select_row, toggle_row},
};
use crate::shared::i18n::text;

pub(super) fn render(
    state: &View,
    view: &gpui_kit::Entity<View>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let prefs = &state.preferences;
    let source_kind = state
        .source
        .state
        .read(cx)
        .selected_value()
        .copied()
        .unwrap_or_else(|| SourceKind::from(&prefs.appearance.source));
    let source_row = select_row(view, Field::Source, &state.source, false, prefs, cx);
    let palette_row = match source_kind {
        SourceKind::System => None,
        SourceKind::Builtin => Some(
            select_row(view, Field::Builtin, &state.builtin, false, prefs, cx).into_any_element(),
        ),
        SourceKind::Custom => Some(
            select_row(
                view,
                Field::Custom,
                &state.custom,
                state.custom_themes_loading || state.custom_themes.is_empty(),
                prefs,
                cx,
            )
            .into_any_element(),
        ),
    };
    let mode_row = select_row(view, Field::Mode, &state.mode, false, prefs, cx);
    let rounded_row = toggle_row(
        view,
        Field::RoundedCorners,
        prefs.appearance.rounded_corners,
        false,
        prefs,
        cx,
    );
    let animations_checked = prefs.animations_enabled.unwrap_or(true);
    let animations_row = toggle_row(
        view,
        Field::Animations,
        animations_checked,
        false,
        prefs,
        cx,
    );
    let content_opacity_row = state.content_opacity.row(
        Field::ContentOpacity,
        format_percent(prefs.appearance.content_opacity),
        prefs,
        view,
        cx,
    );
    let sidebar_opacity_row = state.sidebar_opacity.row(
        Field::SidebarOpacity,
        format_percent(prefs.appearance.sidebar_opacity),
        prefs,
        view,
        cx,
    );
    let font_editor = state.font_editor.clone();
    let sample_text = text("font-sample", cx);
    let sample_family = prefs
        .font
        .family
        .clone()
        .unwrap_or_else(|| cx.theme().font_family.to_string());
    let sample_size = px(prefs.font.size);
    let _ = window;

    el! {
        .div {
            flex; flex_col; w_full; gap_6;
        } [
            $(section_heading(text("settings-appearance", cx), cx)),
            .div { flex; flex_col; w_full; } [
                source_row,
                @if (let Some(row) = palette_row) row, @end,
                mode_row,
                content_opacity_row,
                sidebar_opacity_row,
                rounded_row,
                animations_row,
            ],
            .div { flex; flex_col; w_full; gap_3; } [
                font_editor,
                .div {
                    rounded_md;
                    border_1;
                    border_color: $(cx.theme().border);
                    bg: $(cx.theme().muted.opacity(0.35));
                    px_4;
                    py_3;
                    text_color: $(cx.theme().foreground);
                    font_family: sample_family;
                    text_size: sample_size;
                } [sample_text],
            ],
        ]
    }
}
