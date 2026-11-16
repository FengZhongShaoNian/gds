//! Suite-consistent footer via `Frame::footer`: left index count, clean facts, right zoom controls.

use gds::ui::{Frame, Shortcut, button::IconButton};
use gds_uim::el;
use gpui_kit::{
    Context, IntoElement, SharedString, Window,
    assets::IconName,
    component::{ActiveTheme as _, StyledExt as _},
    div,
    prelude::*,
    px,
};

use super::*;
use crate::{
    entities::catalog::format_byte_size,
    features::navigation::ZoomPreset,
    shared::{
        geometry::FitMode,
        i18n::text,
        ui::actions::{WORKSPACE_CONTEXT, ZoomActual, ZoomFit, ZoomIn, ZoomOut},
    },
};

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn render_footer(
        &mut self,
        frame: &Frame,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let sidebar_status = if self.has_sidebar() && !self.settings.open {
            let label = self
                .session
                .catalog
                .position_display()
                .map(|(cur, total)| format!("{cur} / {total}"))
                .unwrap_or_default();
            el! {
                .div {
                    size_full;
                    flex;
                    items_center;
                    justify_center;
                    text_xs;
                    text_color: $(cx.theme().muted_foreground);
                } [label]
            }
            .into_any_element()
        } else {
            div().into_any_element()
        };

        let left_status = if self.settings.open {
            div().into_any_element()
        } else if let Some(doc) = &self.session.document {
            let (w, h) = doc.output_dimensions();
            let size_part = self
                .session
                .metadata
                .as_ref()
                .and_then(|meta| meta.bundle.technical.file_bytes)
                .map(|bytes| format!(" · {}", format_byte_size(bytes)))
                .unwrap_or_default();
            let summary = format!("{} · {w} × {h} px{size_part}", doc.format().label());
            el! { .div { text_xs; text_color: $(cx.theme().muted_foreground); truncate; } [summary] }
                .into_any_element()
        } else {
            el! {
                .div {
                    text_xs; text_color: $(cx.theme().muted_foreground); truncate;
                } [$(text("empty-subtitle", cx))]
            }
            .into_any_element()
        };

        let right_controls = if !self.settings.open && self.session.document.is_some() {
            let scale = self.canvas.transform.scale;
            let zoom_pct = (scale * 100.0).round() as u32;
            let fit_mode = self.canvas.transform.fit_mode;
            let is_actual = fit_mode == FitMode::ActualPixels
                || (fit_mode == FitMode::Custom && (scale - 1.0).abs() < 0.005);

            let zoom_out = footer_button(
                "footer-zoom-out",
                IconName::ZoomOut,
                text("action-zoom-out", cx),
                Some(Shortcut::new(Box::new(ZoomOut), Some(WORKSPACE_CONTEXT))),
                cx.listener(|this, _, _, cx| this.apply_zoom(ZoomPreset::ZoomOut, cx)),
            );
            let zoom_in = footer_button(
                "footer-zoom-in",
                IconName::ZoomIn,
                text("action-zoom-in", cx),
                Some(Shortcut::new(Box::new(ZoomIn), Some(WORKSPACE_CONTEXT))),
                cx.listener(|this, _, _, cx| this.apply_zoom(ZoomPreset::ZoomIn, cx)),
            );
            let fit_window = footer_button(
                "footer-fit-window",
                IconName::Maximize,
                text("action-zoom-fit", cx),
                Some(Shortcut::new(Box::new(ZoomFit), Some(WORKSPACE_CONTEXT))),
                cx.listener(|this, _, _, cx| this.apply_zoom(ZoomPreset::Fit, cx)),
            )
            .toggled(fit_mode == FitMode::FitToWindow);
            let fit_actual = footer_button(
                "footer-fit-actual",
                IconName::ScanSearch,
                text("action-zoom-actual", cx),
                Some(Shortcut::new(Box::new(ZoomActual), Some(WORKSPACE_CONTEXT))),
                cx.listener(|this, _, _, cx| this.apply_zoom(ZoomPreset::Actual, cx)),
            )
            .toggled(is_actual);
            let fit_width = footer_button(
                "footer-fit-width",
                IconName::UnfoldHorizontal,
                text("action-zoom-fit-width", cx),
                None,
                cx.listener(|this, _, _, cx| this.apply_fit_mode(FitMode::FillWidth, cx)),
            )
            .toggled(fit_mode == FitMode::FillWidth);
            let fit_height = footer_button(
                "footer-fit-height",
                IconName::UnfoldVertical,
                text("action-zoom-fit-height", cx),
                None,
                cx.listener(|this, _, _, cx| this.apply_fit_mode(FitMode::FillHeight, cx)),
            )
            .toggled(fit_mode == FitMode::FillHeight);

            el! {
                .div { flex; items_center; gap_1; } [
                    zoom_out,
                    .div {
                        min_w: $(px(38.0));
                        text_center;
                        text_xs;
                        font_medium;
                        text_color: $(cx.theme().muted_foreground);
                    } [$(format!("{zoom_pct}%"))],
                    zoom_in,
                    .div { w: $(px(1.0)); h: $(px(14.0)); mx_0p5; bg: $(cx.theme().border); },
                    fit_window,
                    fit_actual,
                    fit_width,
                    fit_height,
                ]
            }
            .into_any_element()
        } else {
            div().into_any_element()
        };

        frame
            .footer(sidebar_status, left_status, right_controls, cx)
            .into_any_element()
    }
}

fn footer_button(
    id: &'static str,
    icon: IconName,
    label: SharedString,
    shortcut: Option<Shortcut>,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
) -> IconButton {
    let mut btn = el! {
        id.IconButton::new {
            icon: icon;
            label: label;
            on_click: on_click;
        }
    };
    if let Some(shortcut) = shortcut {
        btn = btn.command(shortcut);
    }
    btn
}
