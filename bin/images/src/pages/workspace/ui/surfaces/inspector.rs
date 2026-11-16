//! Single-column right inspector: resize handle, transforms, tool config popovers, and metadata.

use std::time::Duration;

use gds::{
    service::image::source::Origin,
    ui::{Shortcut, button::Button},
};
use gds_uim::el;
use gpui_kit::{
    Anchor, App, Context, Entity, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    SharedString, TestSupportExt as _, Window,
    assets::IconName,
    component::{
        ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _,
        button::ButtonVariants as _, color_picker::ColorPicker, popover::Popover,
        scroll::ScrollableElement as _,
    },
    div,
    prelude::*,
    px,
};

use super::*;
use crate::{
    entities::{
        catalog::format_byte_size,
        document::RgbaColor,
        preferences::{MAX_INSPECTOR_WIDTH, MIN_INSPECTOR_WIDTH},
    },
    features::edit_canvas::ToolMode,
    shared::{
        i18n::text,
        ui::actions::{
            FlipHorizontal, FlipVertical, RotateCcw, RotateCw, SelectToolArrow, SelectToolBlur,
            SelectToolCrop, SelectToolEllipse, SelectToolLine, SelectToolMosaic, SelectToolPan,
            SelectToolPointer, SelectToolRect, SelectToolText, WORKSPACE_CONTEXT,
        },
    },
};

const TOOL_LONG_PRESS_DELAY: Duration = Duration::from_millis(420);
const SECONDARY_TOOLS: [ToolMode; 9] = [
    ToolMode::Pointer,
    ToolMode::Pan,
    ToolMode::Arrow,
    ToolMode::Rect,
    ToolMode::Ellipse,
    ToolMode::Line,
    ToolMode::Text,
    ToolMode::Mosaic,
    ToolMode::Blur,
];

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn render_inspector(
        &mut self,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let has_doc = self.session.document.is_some();
        let is_busy = self
            .session
            .document
            .as_ref()
            .is_some_and(|doc| self.is_origin_busy(&doc.decoded.origin));

        let edit_section = self.render_edit_section(has_doc, window, cx);
        let props_section = self.render_properties_section(cx);
        let meta_section = self.render_metadata_section(has_doc, is_busy, cx);

        let resize_hover_bg = cx.theme().primary.opacity(0.28);
        let grip_color = if self.chrome.inspector_resizing {
            cx.theme().primary
        } else {
            cx.theme().border
        };

        el! {
            .div {
                # "images-inspector";
                test_support;
                w: $(px(width));
                h_full;
                flex_shrink_0;
                relative;
                flex;
                flex_col;
                border_l_1;
                border_color: $(cx.theme().border);
                overflow_hidden;
            } [
                .div {
                    # "inspector-scroll";
                    flex_1;
                    min_w_0;
                    min_h_0;
                    overflow_y_scrollbar;
                    px_2p5;
                    py_3;
                    pl: $(px(12.0));
                    flex;
                    flex_col;
                    gap_2p5;
                } [
                    edit_section,
                    $(section_divider(cx)),
                    props_section,
                    $(section_divider(cx)),
                    meta_section,
                ],
                .div {
                    # "inspector-resize-handle";
                    absolute;
                    left_0;
                    top_0;
                    bottom_0;
                    w: $(px(6.0));
                    flex;
                    items_center;
                    justify_center;
                    cursor_col_resize;
                    hover: $(move |style| style.bg(resize_hover_bg));
                    on_mouse_down: MouseButton::Left, $(cx.listener(Self::begin_inspector_resize));
                } [
                    .div { w: $(px(2.0)); h: $(px(28.0)); rounded_full; bg: grip_color; },
                ],
            ]
        }
    }

    fn begin_inspector_resize(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.chrome.inspector_resizing = true;
        self.chrome.inspector_drag_anchor =
            Some((f32::from(event.position.x), self.chrome.inspector_width));
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn on_inspector_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.chrome.inspector_resizing {
            return;
        }
        let Some((anchor_x, anchor_w)) = self.chrome.inspector_drag_anchor else {
            return;
        };
        let win_w = f32::from(window.bounds().size.width);
        let max_allowed = ((win_w - 300.0).max(f32::from(MIN_INSPECTOR_WIDTH)))
            .min(f32::from(MAX_INSPECTOR_WIDTH));
        let delta = anchor_x - f32::from(event.position.x);
        let next = (f32::from(anchor_w) + delta)
            .clamp(f32::from(MIN_INSPECTOR_WIDTH), max_allowed)
            .round() as u16;
        if next != self.chrome.inspector_width {
            self.chrome.inspector_width = next;
            cx.notify();
        }
    }

    pub(in crate::pages::workspace::ui) fn end_inspector_resize(&mut self, cx: &mut Context<Self>) {
        if self.chrome.inspector_resizing || self.chrome.inspector_drag_anchor.is_some() {
            self.chrome.inspector_resizing = false;
            self.chrome.inspector_drag_anchor = None;
            self.persist_chrome_preferences(cx);
            cx.notify();
        }
    }

    fn render_edit_section(
        &self,
        has_doc: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let transforms: [(&'static str, IconName, &'static str, Shortcut, TransformOp); 4] = [
            (
                "insp-rotate-ccw",
                IconName::RotateCcw,
                "action-rotate-ccw",
                Shortcut::new(Box::new(RotateCcw), Some(WORKSPACE_CONTEXT)),
                TransformOp::RotateCcw,
            ),
            (
                "insp-rotate-cw",
                IconName::RotateCw,
                "action-rotate-cw",
                Shortcut::new(Box::new(RotateCw), Some(WORKSPACE_CONTEXT)),
                TransformOp::RotateCw,
            ),
            (
                "insp-flip-h",
                IconName::FlipHorizontal2,
                "action-flip-h",
                Shortcut::new(Box::new(FlipHorizontal), Some(WORKSPACE_CONTEXT)),
                TransformOp::FlipH,
            ),
            (
                "insp-flip-v",
                IconName::FlipVertical2,
                "action-flip-v",
                Shortcut::new(Box::new(FlipVertical), Some(WORKSPACE_CONTEXT)),
                TransformOp::FlipV,
            ),
        ];

        let mut primary_row = transforms
            .into_iter()
            .map(|(id, icon, label_id, shortcut, op)| {
                el! {
                    id.gds::ui::button::IconButton::new {
                        icon: icon;
                        label: $(text(label_id, cx));
                        command: shortcut;
                        disabled: $(!has_doc);
                        on_click: $(cx.listener(move |this, _, _, cx| this.apply_transform_op(op, cx)));
                    }
                }
                .into_any_element()
            })
            .collect::<Vec<_>>();
        primary_row.push(self.render_tool_button(ToolMode::Crop, has_doc, window, cx));

        let tool_buttons = SECONDARY_TOOLS
            .into_iter()
            .map(|mode| self.render_tool_button(mode, has_doc, window, cx))
            .collect::<Vec<_>>();

        el! {
            .div { # "inspector-edit"; test_support; flex; flex_col; gap_2; } [
                $(section_heading(text("inspector-edit", cx), cx)),
                .div { flex; items_center; justify_between; gap_1; } [..primary_row],
                .div { flex; flex_wrap; gap_1; } [..tool_buttons],
            ]
        }
    }

    fn render_tool_button(
        &self,
        mode: ToolMode,
        has_doc: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let active_tool = self.canvas.tool_fsm.tool;
        let open_popover = self.canvas.tool_popover;
        let id: gpui_kit::ElementId = mode.element_id().into();
        let active = active_tool == mode;
        let configurable = mode.is_configurable();
        let icon = tool_icon(mode);
        let label = text(mode.label_key(), cx);
        let shortcut = tool_shortcut(mode);

        if !configurable {
            return el! {
                id.gds::ui::button::IconButton::new {
                    icon: icon;
                    label: label;
                    command: shortcut;
                    disabled: $(!has_doc);
                    active: active;
                    on_click: $(cx.listener(move |this, _, window, cx| {
                        this.canvas.tool_popover = None;
                        this.select_tool(mode, window, cx);
                    }));
                }
            }
            .into_any_element();
        }

        let handle = window.window_handle();
        let mut trigger = el! {
            id.Button::icon_button {
                control_icon: icon;
                accessibility_label: $(label.clone());
                tooltip_with_shortcut: label, shortcut;
                disabled: $(!has_doc);
                toggled: active;
                on_mouse_down: MouseButton::Left, $(cx.listener(move |this, _, window, cx| {
                    this.select_tool(mode, window, cx);
                    let view_ref = cx.entity().downgrade();
                    this.canvas.long_press_task = Some(cx.spawn(async move |_, cx| {
                        cx.background_executor().timer(TOOL_LONG_PRESS_DELAY).await;
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = view_ref.update(cx, |this, cx| {
                                let current = color_to_hsla(this.canvas.tool_fsm.settings.color);
                                this.focus.color_picker.update(cx, |picker, cx| {
                                    picker.set_open(false, cx);
                                    picker.set_value(current, window, cx);
                                });
                                this.canvas.tool_popover = Some(mode);
                                cx.notify();
                            });
                        });
                    }));
                }));
                on_mouse_up: MouseButton::Left, $(cx.listener(|this, _, _, _| {
                    this.canvas.long_press_task = None;
                }));
                on_mouse_up_out: MouseButton::Left, $(cx.listener(|this, _, _, _| {
                    this.canvas.long_press_task = None;
                }));
            }
        };
        if active {
            trigger = trigger.primary();
        }

        let is_open = open_popover == Some(mode);
        let picker_open =
            self.focus.color_picker.read(cx).is_open() || self.canvas.color_picker_open;
        let pop_id: gpui_kit::ElementId = ("insp-pop", mode as usize).into();
        let view_ref = cx.entity().downgrade();
        let content_view = view_ref.clone();

        el! {
            pop_id.Popover::new {
                anchor: Anchor::TopRight;
                mouse_button: MouseButton::Right;
                appearance: true;
                overlay_closable: $(!picker_open);
                open: is_open;
                trigger: trigger;
                on_open_change: $(move |next: &bool, window, cx| {
                    let _ = view_ref.update(cx, |this, cx| {
                        if *next {
                            let current = color_to_hsla(this.canvas.tool_fsm.settings.color);
                            this.focus.color_picker.update(cx, |picker, cx| {
                                picker.set_open(false, cx);
                                picker.set_value(current, window, cx);
                            });
                            this.select_tool(mode, window, cx);
                            this.canvas.tool_popover = Some(mode);
                            cx.notify();
                        } else if !this.focus.color_picker.read(cx).is_open()
                            && this.canvas.tool_popover == Some(mode)
                        {
                            this.canvas.tool_popover = None;
                            this.canvas.color_picker_open = false;
                            cx.notify();
                        }
                    });
                });
                content: $(move |_, _, cx| {
                    content_view
                        .upgrade()
                        .map(|entity| render_tool_popover_content(&entity, mode, cx))
                        .unwrap_or_else(|| el! { .div }.into_any_element())
                });
            }
        }
        .into_any_element()
    }

    fn render_properties_section(&self, cx: &App) -> impl IntoElement + use<> {
        let Some(doc) = &self.session.document else {
            return el! {
                .div { flex; flex_col; gap_1p5; } [
                    $(section_heading(text("inspector-properties", cx), cx)),
                    .div { text_xs; text_color: $(cx.theme().muted_foreground); } [$(text("empty-title", cx))],
                ]
            }
            .into_any_element();
        };
        let Some(meta) = &self.session.metadata else {
            return el! {
                .div { flex; flex_col; gap_1p5; } [
                    $(section_heading(text("inspector-properties", cx), cx)),
                ]
            }
            .into_any_element();
        };

        let tech = &meta.bundle.technical;
        let (out_w, out_h) = doc.output_dimensions();
        let dims: SharedString = format!("{out_w} × {out_h} px").into();
        let file_size: SharedString = tech
            .file_bytes
            .map(format_byte_size)
            .unwrap_or_else(|| "—".to_owned())
            .into();
        let profile: SharedString = tech
            .icc_profile_name
            .clone()
            .unwrap_or_else(|| {
                if tech.has_icc_profile {
                    "ICC".to_owned()
                } else {
                    "sRGB".to_owned()
                }
            })
            .into();

        let exif = &meta.bundle.exif;
        let camera = [exif.camera_make.as_deref(), exif.camera_model.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let exposure = [exif.exposure_time.as_deref(), exif.f_number.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");

        el! {
            .div { flex; flex_col; gap_1p5; } [
                $(section_heading(text("inspector-properties", cx), cx)),
                $(fact_row(text("field-format", cx), doc.format().label().into(), cx)),
                $(fact_row(text("field-dimensions", cx), dims, cx)),
                $(fact_row(text("field-file-size", cx), file_size, cx)),
                $(fact_row(text("field-color-type", cx), tech.color_type.clone().into(), cx)),
                $(fact_row(text("field-color-profile", cx), profile, cx)),
                @if (tech.orientation > 1)
                    $(fact_row(text("field-orientation", cx), tech.orientation.to_string().into(), cx)),
                @end,
                @if (!exif.is_empty())
                    .div { pt_1; flex; flex_col; gap_1; } [
                        .div {
                            text_xs; font_semibold; text_color: $(cx.theme().foreground);
                        } [$(text("inspector-camera", cx))],
                        @if (!camera.is_empty())
                            $(fact_row(text("field-camera", cx), camera.into(), cx)),
                        @end,
                        @if (let Some(lens) = &exif.lens_model)
                            $(fact_row(text("field-lens", cx), lens.clone().into(), cx)),
                        @end,
                        @if (!exposure.is_empty())
                            $(fact_row(text("field-exposure", cx), exposure.into(), cx)),
                        @end,
                        @if (let Some(iso) = exif.iso)
                            $(fact_row(text("field-iso", cx), iso.to_string().into(), cx)),
                        @end,
                        @if (let Some(focal) = &exif.focal_length)
                            $(fact_row(text("field-focal-length", cx), focal.clone().into(), cx)),
                        @end,
                    ],
                @end,
            ]
        }
        .into_any_element()
    }

    fn render_metadata_section(
        &self,
        has_doc: bool,
        is_busy: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let can_clear = has_doc
            && !is_busy
            && self
                .session
                .document
                .as_ref()
                .is_some_and(|doc| matches!(doc.decoded.origin, Origin::LocalFile { .. }));
        el! {
            .div { # "inspector-metadata"; test_support; flex; flex_col; gap_2; } [
                $(section_heading(text("inspector-metadata", cx), cx)),
                "insp-clear-privacy".Button::new {
                    small;
                    w_full;
                    disabled: $(!can_clear);
                    icon: IconName::ShieldCheck;
                    label: $(text("inspector-strip-privacy", cx));
                    on_click: $(cx.listener(|this, _, window, cx| this.clear_privacy_metadata(window, cx)));
                },
            ]
        }
    }
}

fn section_heading(title: SharedString, cx: &App) -> impl IntoElement + use<> {
    el! {
        .div {
            text_xs;
            font_semibold;
            text_color: $(cx.theme().muted_foreground);
        } [title]
    }
}

fn section_divider(cx: &App) -> impl IntoElement + use<> {
    el! { .div { w_full; h: $(px(1.0)); bg: $(cx.theme().border); } }
}

fn fact_row(label: SharedString, value: SharedString, cx: &App) -> impl IntoElement + use<> {
    el! {
        .div {
            flex; items_center; justify_between; gap_2; text_xs; min_w_0;
        } [
            .div { text_color: $(cx.theme().muted_foreground); flex_shrink_0; } [label],
            .div { text_color: $(cx.theme().foreground); truncate; } [value],
        ]
    }
}

const fn tool_icon(mode: ToolMode) -> IconName {
    match mode {
        ToolMode::Pointer => IconName::MousePointer,
        ToolMode::Pan => IconName::Hand,
        ToolMode::Crop => IconName::Crop,
        ToolMode::Rect => IconName::Square,
        ToolMode::Ellipse => IconName::Circle,
        ToolMode::Arrow => IconName::ArrowUpRight,
        ToolMode::Line => IconName::Minus,
        ToolMode::Text => IconName::Type,
        ToolMode::Mosaic => IconName::Grid2x2,
        ToolMode::Blur => IconName::Droplet,
    }
}

fn tool_shortcut(mode: ToolMode) -> Shortcut {
    match mode {
        ToolMode::Pointer => Shortcut::new(Box::new(SelectToolPointer), Some(WORKSPACE_CONTEXT)),
        ToolMode::Pan => Shortcut::new(Box::new(SelectToolPan), Some(WORKSPACE_CONTEXT)),
        ToolMode::Crop => Shortcut::new(Box::new(SelectToolCrop), Some(WORKSPACE_CONTEXT)),
        ToolMode::Rect => Shortcut::new(Box::new(SelectToolRect), Some(WORKSPACE_CONTEXT)),
        ToolMode::Ellipse => Shortcut::new(Box::new(SelectToolEllipse), Some(WORKSPACE_CONTEXT)),
        ToolMode::Arrow => Shortcut::new(Box::new(SelectToolArrow), Some(WORKSPACE_CONTEXT)),
        ToolMode::Line => Shortcut::new(Box::new(SelectToolLine), Some(WORKSPACE_CONTEXT)),
        ToolMode::Text => Shortcut::new(Box::new(SelectToolText), Some(WORKSPACE_CONTEXT)),
        ToolMode::Mosaic => Shortcut::new(Box::new(SelectToolMosaic), Some(WORKSPACE_CONTEXT)),
        ToolMode::Blur => Shortcut::new(Box::new(SelectToolBlur), Some(WORKSPACE_CONTEXT)),
    }
}

fn render_tool_popover_content(
    entity: &Entity<WorkspacePage>,
    mode: ToolMode,
    cx: &App,
) -> gpui_kit::AnyElement {
    let page = entity.read(cx);
    let settings = page.canvas.tool_fsm.settings;
    let picker_state = page.focus.color_picker.clone();
    let uses_color = mode.uses_color();
    let view = entity.downgrade();

    let swatches = RgbaColor::PALETTE
        .into_iter()
        .enumerate()
        .map(|(i, color)| {
            let selected = settings.color == color;
            let swatch_id: gpui_kit::ElementId = ("tool-swatch", i).into();
            let v = view.clone();
            el! {
                .div {
                    # swatch_id;
                    size: $(px(18.0));
                    rounded_full;
                    cursor_pointer;
                    border_2;
                    border_color: $(if selected { cx.theme().primary } else { cx.theme().border });
                    bg: $(color_to_hsla(color));
                    on_mouse_down: MouseButton::Left, $(move |_, _, cx| {
                        let _ = v.update(cx, |this, cx| this.set_tool_color(color, cx));
                    });
                }
            }
        })
        .collect::<Vec<_>>();

    let size_chips = match mode {
        ToolMode::Text => [
            (14.0_f32, "size-small"),
            (20.0, "size-medium"),
            (30.0, "size-large"),
        ]
        .into_iter()
        .enumerate()
        .map(|(idx, (sz, key))| {
            let selected = (settings.font_size - sz).abs() < 0.5;
            let v = view.clone();
            preset_chip(
                ("tool-font-preset", idx).into(),
                selected,
                text(key, cx),
                move |_, _, cx| {
                    let _ = v.update(cx, |this, cx| this.set_tool_font_size(sz, cx));
                },
            )
        })
        .collect::<Vec<_>>(),
        ToolMode::Mosaic => [
            (8_u32, "size-small"),
            (14, "size-medium"),
            (22, "size-large"),
        ]
        .into_iter()
        .enumerate()
        .map(|(idx, (sz, key))| {
            let selected = settings.mosaic_block_size == sz;
            let v = view.clone();
            preset_chip(
                ("tool-mosaic-preset", idx).into(),
                selected,
                text(key, cx),
                move |_, _, cx| {
                    let _ = v.update(cx, |this, cx| this.set_tool_mosaic_block(sz, cx));
                },
            )
        })
        .collect::<Vec<_>>(),
        ToolMode::Blur => [
            (4_u32, "size-small"),
            (8, "size-medium"),
            (14, "size-large"),
        ]
        .into_iter()
        .enumerate()
        .map(|(idx, (sz, key))| {
            let selected = settings.blur_radius == sz;
            let v = view.clone();
            preset_chip(
                ("tool-blur-preset", idx).into(),
                selected,
                text(key, cx),
                move |_, _, cx| {
                    let _ = v.update(cx, |this, cx| this.set_tool_blur_radius(sz, cx));
                },
            )
        })
        .collect::<Vec<_>>(),
        _ => [
            (2.0_f32, "size-small"),
            (3.0, "size-medium"),
            (6.0, "size-large"),
        ]
        .into_iter()
        .enumerate()
        .map(|(idx, (sz, key))| {
            let selected = (settings.stroke_width - sz).abs() < 0.5;
            let v = view.clone();
            preset_chip(
                ("tool-stroke-preset", idx).into(),
                selected,
                text(key, cx),
                move |_, _, cx| {
                    let _ = v.update(cx, |this, cx| this.set_tool_stroke_width(sz, cx));
                },
            )
        })
        .collect::<Vec<_>>(),
    };

    let picker_open_view = view.clone();
    el! {
        .div {
            flex; flex_col; gap_2; p_2; min_w: $(px(184.0));
        } [
            @if (uses_color)
                .div { flex; flex_col; gap_2; } [
                    .div { flex; items_center; gap_1p5; } [..swatches],
                    .div {
                        flex; items_center; justify_between; gap_2;
                        on_mouse_down: MouseButton::Left, $(move |_, _, cx| {
                            let _ = picker_open_view.update(cx, |this, cx| {
                                this.canvas.color_picker_open = true;
                                cx.notify();
                            });
                        });
                    } [
                        .div { text_xs; text_color: $(cx.theme().muted_foreground); } [$(text("tool-custom-color", cx))],
                        $(ColorPicker::new(&picker_state).small()),
                    ],
                ],
            @end,
            .div { flex; items_center; gap_1; } [..size_chips],
        ]
    }
    .into_any_element()
}

fn preset_chip(
    id: gpui_kit::ElementId,
    selected: bool,
    label: SharedString,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    let mut btn = el! {
        id.Button::new {
            small;
            label: label;
            on_click: on_click;
        }
    };
    if selected {
        btn = btn.primary();
    }
    btn
}
