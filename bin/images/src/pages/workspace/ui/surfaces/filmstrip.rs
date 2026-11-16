//! Left vertical filmstrip sidebar matching suite widths, centered items, and LIFO thumbnails.

use std::sync::Arc;

use gds::{
    service::image::source::Origin,
    ui::{
        button::IconButton, file_inspection::State as InspectionState,
        sidebar::SidebarPresentation, thumbnail::Thumbnail,
    },
};
use gds_uim::el;
use gpui_kit::{
    AnyElement, Bounds, Context, IntoElement, MouseButton, ObjectFit, Pixels, ScrollDelta,
    ScrollEvent, TestSupportExt as _, WeakEntity, Window,
    assets::IconName,
    canvas,
    component::{ActiveTheme as _, StyledExt as _},
    div, img,
    prelude::*,
    px,
};

use super::*;
use crate::{
    entities::catalog::CatalogEntry,
    pages::workspace::ui::navigation::catalog::entry_inspection_source, shared::i18n::text,
};

const THUMB_SIZE: f32 = 38.0;
const ROW_H_COLLAPSED: f32 = THUMB_SIZE + 4.0;
const ROW_H_EXPANDED: f32 = 54.0;
const ROW_GAP: f32 = 4.0;
const CHEVRON_OVERHEAD: f32 = 84.0;
const DEFAULT_TRACK_HEIGHT: f32 = 480.0;
const SCROLL_STEP_THRESHOLD: f32 = 36.0;

#[derive(Clone, Copy)]
struct FilmstripCardStyle {
    is_active: bool,
    row_h: f32,
    progress: f32,
    show_labels: bool,
}

fn capacity_for_height(track_height: f32, row_height: f32) -> usize {
    let usable = (track_height - CHEVRON_OVERHEAD).max(row_height);
    let step = row_height + ROW_GAP;
    (((usable + ROW_GAP) / step).floor() as usize).max(1)
}

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn render_filmstrip(
        &mut self,
        presentation: SidebarPresentation,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let width = presentation.width();
        let progress = presentation.progress();
        let show_labels = progress > 0.05;
        let track_h = f32::from(self.chrome.filmstrip_bounds.get().size.height);
        let effective_h = if track_h > 1.0 {
            track_h
        } else {
            DEFAULT_TRACK_HEIGHT
        };
        let target_row_h = if self.chrome.filmstrip_expanded {
            ROW_H_EXPANDED
        } else {
            ROW_H_COLLAPSED
        };
        let capacity = capacity_for_height(effective_h, target_row_h);

        let bounds_cell = self.chrome.filmstrip_bounds.clone();
        let weak_view: WeakEntity<Self> = cx.entity().downgrade();
        let measure = canvas(
            move |new_bounds: Bounds<Pixels>, _, cx| {
                let prev = bounds_cell.replace(new_bounds);
                let prev_h = f32::from(prev.size.height);
                let next_h = f32::from(new_bounds.size.height);
                let prev_cap = capacity_for_height(
                    if prev_h > 1.0 {
                        prev_h
                    } else {
                        DEFAULT_TRACK_HEIGHT
                    },
                    target_row_h,
                );
                let next_cap = capacity_for_height(
                    if next_h > 1.0 {
                        next_h
                    } else {
                        DEFAULT_TRACK_HEIGHT
                    },
                    target_row_h,
                );
                if prev_cap != next_cap {
                    let _ = weak_view.update(cx, |_, cx| cx.notify());
                }
            },
            |_, (), _, _| {},
        )
        .absolute()
        .inset_0();

        let prev_btn = el! {
            .div { # "filmstrip-prev-slot"; test_support; w_full; flex; justify_center; flex_shrink_0; } [
                "filmstrip-prev".IconButton::new {
                    icon: IconName::ChevronUp;
                    label: $(text("action-prev", cx));
                    disabled: $(!self.session.catalog.can_step_prev());
                    on_click: $(cx.listener(|this, _, window, cx| this.navigate_sibling(-1, window, cx)));
                },
            ]
        };
        let next_btn = el! {
            .div { # "filmstrip-next-slot"; test_support; w_full; flex; justify_center; flex_shrink_0; } [
                "filmstrip-next".IconButton::new {
                    icon: IconName::ChevronDown;
                    label: $(text("action-next", cx));
                    disabled: $(!self.session.catalog.can_step_next());
                    on_click: $(cx.listener(|this, _, window, cx| this.navigate_sibling(1, window, cx)));
                },
            ]
        };

        let row_h = presentation.interpolate(ROW_H_EXPANDED, ROW_H_COLLAPSED);
        let total = self.session.catalog.entries.len();
        let (start_idx, visible) = self.session.catalog.visible_window(capacity);
        let style = FilmstripCardStyle {
            is_active: false,
            row_h,
            progress,
            show_labels,
        };
        let rows: Vec<AnyElement> = if !visible.is_empty() {
            visible
                .iter()
                .enumerate()
                .map(|(offset, entry)| {
                    let idx = start_idx + offset;
                    let card_style = FilmstripCardStyle {
                        is_active: self.session.catalog.current_index == Some(idx),
                        ..style
                    };
                    self.render_filmstrip_card(idx, entry, card_style, cx)
                        .into_any_element()
                })
                .collect()
        } else if let Some(doc) = &self.session.document {
            vec![self.render_memory_filmstrip_card(doc, style, cx)]
        } else {
            Vec::new()
        };

        let has_rows = !rows.is_empty();
        let is_overflow = has_rows && total > visible.len();
        let pad_x = 9.0 * (1.0 - progress) + 8.0 * progress;

        let track = if is_overflow {
            el! {
                .div {
                    # "images-filmstrip-track";
                    flex_1;
                    min_h_0;
                    w_full;
                    overflow_hidden;
                    flex;
                    flex_col;
                    justify_center;
                    gap: $(px(ROW_GAP));
                } [..rows]
            }
        } else {
            el! {
                .div {
                    # "images-filmstrip-track";
                    w_full;
                    flex;
                    flex_col;
                    gap: $(px(ROW_GAP));
                } [..rows]
            }
        };

        let mut root = el! {
            .div {
                # "images-filmstrip";
                test_support;
                relative;
                w: $(px(width));
                h_full;
                flex_shrink_0;
                flex;
                flex_col;
                items_center;
                py_2;
                pl: $(px(pad_x));
                pr: $(px(pad_x));
                overflow_hidden;
                on_scroll: $(cx.listener(Self::on_filmstrip_scroll));
            }
        };
        if is_overflow {
            root = root.justify_between();
        } else {
            root = root.justify_center().gap_1p5();
        }

        el! {
            root [
                measure,
                @if (has_rows) prev_btn, @end,
                track,
                @if (has_rows) next_btn, @end,
            ]
        }
    }

    fn render_filmstrip_card(
        &self,
        index: usize,
        entry: &CatalogEntry,
        style: FilmstripCardStyle,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let thumb = self.filmstrip_thumb_element(entry, style.is_active, cx);
        let path = entry.path.clone();
        let display_name = entry.meta.display_name.clone();
        let subtitle = entry.meta.subtitle.clone();

        let card_bg = if style.is_active {
            cx.theme().selection
        } else {
            gpui_kit::transparent_black()
        };
        let border_col = if style.is_active {
            cx.theme().primary
        } else {
            gpui_kit::transparent_black()
        };
        let hover_bg = cx.theme().list_hover;
        let fg = cx.theme().foreground;
        let sub_fg = cx.theme().muted_foreground;

        el! {
            .div {
                # $(("filmstrip-item", index));
                h: $(px(style.row_h));
                w_full;
                flex;
                flex_row;
                items_center;
                gap_2;
                px: $(px(4.0 * style.progress));
                rounded_md;
                border_1;
                border_color: border_col;
                bg: card_bg;
                hover: $(move |s| s.bg(hover_bg));
                cursor_pointer;
                overflow_hidden;
                on_mouse_down: MouseButton::Left, $(cx.listener(move |this, _, window, cx| {
                    this.open_local_with_confirm(path.clone(), window, cx);
                }));
            } [
                .div {
                    size: $(px(THUMB_SIZE));
                    flex_shrink_0;
                    rounded_sm;
                    overflow_hidden;
                    bg: $(cx.theme().muted.opacity(0.45));
                    flex;
                    items_center;
                    justify_center;
                } [thumb],
                @if (style.show_labels)
                    .div {
                        flex_1;
                        min_w_0;
                        flex;
                        flex_col;
                        opacity: $(style.progress);
                    } [
                        .div {
                            text_xs;
                            font_medium;
                            text_color: fg;
                            truncate;
                        } [display_name],
                        .div {
                            text_xs;
                            text_color: sub_fg;
                            truncate;
                        } [subtitle],
                    ],
                @end,
            ]
        }
    }

    fn render_memory_filmstrip_card(
        &self,
        doc: &crate::entities::document::DocumentState,
        style: FilmstripCardStyle,
        cx: &App,
    ) -> AnyElement {
        let thumb = if let Some(crate::entities::document::RenderOutput::Single { image }) =
            &doc.cache.output
        {
            img(Arc::clone(image))
                .object_fit(ObjectFit::Cover)
                .size_full()
                .into_any_element()
        } else {
            div().size_full().into_any_element()
        };
        let (w, h) = doc.output_dimensions();
        let title: gpui_kit::SharedString = origin_display_name(&doc.decoded.origin).into();
        let subtitle: gpui_kit::SharedString = format!("{} · {w}×{h}", doc.format().label()).into();

        el! {
            .div {
                # "filmstrip-item-memory";
                h: $(px(style.row_h));
                w_full;
                flex;
                flex_row;
                items_center;
                gap_2;
                px: $(px(4.0 * style.progress));
                rounded_md;
                border_1;
                border_color: $(cx.theme().primary);
                bg: $(cx.theme().selection);
                overflow_hidden;
            } [
                .div {
                    size: $(px(THUMB_SIZE));
                    flex_shrink_0;
                    rounded_sm;
                    overflow_hidden;
                    bg: $(cx.theme().muted.opacity(0.45));
                    flex;
                    items_center;
                    justify_center;
                } [thumb],
                @if (style.show_labels)
                    .div {
                        flex_1;
                        min_w_0;
                        flex;
                        flex_col;
                        opacity: $(style.progress);
                    } [
                        .div { text_xs; font_medium; text_color: $(cx.theme().foreground); truncate; } [title],
                        .div { text_xs; text_color: $(cx.theme().muted_foreground); truncate; } [subtitle],
                    ],
                @end,
            ]
        }
        .into_any_element()
    }

    fn filmstrip_thumb_element(
        &self,
        entry: &CatalogEntry,
        is_active: bool,
        cx: &App,
    ) -> impl IntoElement + use<> {
        let source = entry_inspection_source(entry);
        // Fast-path cached ready thumbnails directly without triggering cache churn every frame.
        if let Some(snap) = self.runtime.inspections.read(cx).get(&source)
            && let InspectionState::Ready(image) = &snap.thumbnail
        {
            return img(Arc::clone(image))
                .object_fit(ObjectFit::Cover)
                .size_full()
                .into_any_element();
        }
        // Guard the active slot during pending switches so stale decoded pixels never flash across slots.
        if is_active
            && !self.runtime.is_loading
            && let Some(doc) = &self.session.document
            && matches!(&doc.decoded.origin, Origin::LocalFile { path, .. } if *path == entry.path)
            && let Some(crate::entities::document::RenderOutput::Single { image }) =
                &doc.cache.output
        {
            return img(Arc::clone(image))
                .object_fit(ObjectFit::Cover)
                .size_full()
                .into_any_element();
        }
        Thumbnail::new(self.runtime.inspections.clone(), source)
            .show_status(false)
            .into_any_element()
    }

    fn on_filmstrip_scroll(
        &mut self,
        event: &ScrollEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.catalog.entries.len() <= 1 {
            return;
        }
        let delta_y = match event.delta {
            ScrollDelta::Pixels(p) => f32::from(p.y),
            ScrollDelta::Lines(l) => l.y * SCROLL_STEP_THRESHOLD,
        };
        if !delta_y.is_finite() || delta_y == 0.0 {
            return;
        }
        if self.chrome.filmstrip_scroll_accum_y != 0.0
            && self.chrome.filmstrip_scroll_accum_y.signum() != delta_y.signum()
        {
            self.chrome.filmstrip_scroll_accum_y = 0.0;
        }
        self.chrome.filmstrip_scroll_accum_y += delta_y;
        if self.chrome.filmstrip_scroll_accum_y.abs() >= SCROLL_STEP_THRESHOLD {
            let step = if self.chrome.filmstrip_scroll_accum_y < 0.0 {
                1
            } else {
                -1
            };
            self.chrome.filmstrip_scroll_accum_y %= SCROLL_STEP_THRESHOLD;
            self.navigate_sibling(step, window, cx);
        }
    }
}
