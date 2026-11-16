//! Pointer/crop/annotation interaction finite-state machine.

use std::sync::Arc;

use gds::service::image::PixelRect;

use crate::{
    entities::document::{AnnotationItem, AnnotationKind, DocumentState, RgbaColor},
    shared::geometry::{Point2, Size2, ViewportTransform},
};

use super::{command::EditCommand, history::EditHistory};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, strum::EnumIter)]
pub enum ToolMode {
    #[default]
    Pointer,
    Pan,
    Crop,
    Rect,
    Ellipse,
    Arrow,
    Line,
    Text,
    Mosaic,
    Blur,
}

impl ToolMode {
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Pointer => "tool-select",
            Self::Pan => "tool-pan",
            Self::Crop => "tool-crop",
            Self::Rect => "tool-rect",
            Self::Ellipse => "tool-ellipse",
            Self::Arrow => "tool-arrow",
            Self::Line => "tool-line",
            Self::Text => "tool-text",
            Self::Mosaic => "tool-mosaic",
            Self::Blur => "tool-blur",
        }
    }

    pub const fn element_id(self) -> &'static str {
        self.label_key()
    }

    pub const fn is_configurable(self) -> bool {
        matches!(
            self,
            Self::Rect
                | Self::Ellipse
                | Self::Arrow
                | Self::Line
                | Self::Text
                | Self::Mosaic
                | Self::Blur
        )
    }

    pub const fn uses_color(self) -> bool {
        matches!(
            self,
            Self::Rect | Self::Ellipse | Self::Arrow | Self::Line | Self::Text
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolSettings {
    pub color: RgbaColor,
    pub stroke_width: f32,
    pub font_size: f32,
    pub mosaic_block_size: u32,
    pub blur_radius: u32,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            color: RgbaColor::RED,
            stroke_width: 3.0,
            font_size: 24.0,
            mosaic_block_size: 14,
            blur_radius: 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragPreview {
    pub tool: ToolMode,
    pub start: Point2,
    pub current: Point2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FinishOutcome {
    None,
    CropStaged(PixelRect),
    Committed,
}

#[derive(Default)]
pub struct ToolFsm {
    pub tool: ToolMode,
    pub settings: ToolSettings,
    pub drag: Option<DragPreview>,
    pub staged_crop: Option<PixelRect>,
    pub pending_text_pos: Option<Point2>,
}

impl ToolFsm {
    pub fn set_tool(&mut self, tool: ToolMode) {
        self.tool = tool;
        self.drag = None;
        if tool != ToolMode::Text {
            self.pending_text_pos = None;
        }
        if tool != ToolMode::Crop {
            self.staged_crop = None;
        }
    }

    pub fn begin_drag(&mut self, img_pt: Point2) {
        match self.tool {
            ToolMode::Pointer | ToolMode::Pan => {
                self.drag = None;
            }
            ToolMode::Text => {
                self.drag = None;
                self.pending_text_pos = Some(img_pt);
            }
            other => {
                self.drag = Some(DragPreview {
                    tool: other,
                    start: img_pt,
                    current: img_pt,
                });
            }
        }
    }

    pub fn update_drag(&mut self, img_pt: Point2) {
        if let Some(drag) = &mut self.drag {
            drag.current = img_pt;
        }
    }

    pub fn finish_drag(
        &mut self,
        doc: &mut DocumentState,
        history: &mut EditHistory,
    ) -> FinishOutcome {
        let Some(drag) = self.drag.take() else {
            return FinishOutcome::None;
        };
        let (out_w, out_h) = doc.output_dimensions();
        let img_size = Size2::new(out_w as f32, out_h as f32);
        let dx = (drag.current.x - drag.start.x).abs();
        let dy = (drag.current.y - drag.start.y).abs();
        if dx < 2.0 && dy < 2.0 {
            return FinishOutcome::None;
        }

        let norm = ViewportTransform::normalize_drag_rect(drag.start, drag.current);
        let kind = match drag.tool {
            ToolMode::Crop => {
                let Some(rect) = ViewportTransform::clamp_image_rect(norm, img_size) else {
                    return FinishOutcome::None;
                };
                if rect.width < 2 || rect.height < 2 {
                    return FinishOutcome::None;
                }
                self.staged_crop = Some(rect);
                return FinishOutcome::CropStaged(rect);
            }
            ToolMode::Rect => {
                let Some(rect) = ViewportTransform::clamp_image_rect(norm, img_size) else {
                    return FinishOutcome::None;
                };
                AnnotationKind::Rect {
                    rect,
                    stroke: self.settings.color,
                    stroke_width: self.settings.stroke_width,
                    fill: None,
                }
            }
            ToolMode::Ellipse => {
                let Some(rect) = ViewportTransform::clamp_image_rect(norm, img_size) else {
                    return FinishOutcome::None;
                };
                AnnotationKind::Ellipse {
                    rect,
                    stroke: self.settings.color,
                    stroke_width: self.settings.stroke_width,
                    fill: None,
                }
            }
            ToolMode::Arrow => {
                let Some((start, end)) = clip_segment_to_canvas(drag.start, drag.current, img_size)
                else {
                    return FinishOutcome::None;
                };
                AnnotationKind::Arrow {
                    start,
                    end,
                    color: self.settings.color,
                    stroke_width: self.settings.stroke_width,
                }
            }
            ToolMode::Line => {
                let Some((start, end)) = clip_segment_to_canvas(drag.start, drag.current, img_size)
                else {
                    return FinishOutcome::None;
                };
                AnnotationKind::Line {
                    start,
                    end,
                    color: self.settings.color,
                    stroke_width: self.settings.stroke_width,
                }
            }
            ToolMode::Mosaic => {
                let Some(region) = ViewportTransform::clamp_image_rect(norm, img_size) else {
                    return FinishOutcome::None;
                };
                AnnotationKind::Mosaic {
                    region,
                    block_size: self.settings.mosaic_block_size,
                }
            }
            ToolMode::Blur => {
                let Some(region) = ViewportTransform::clamp_image_rect(norm, img_size) else {
                    return FinishOutcome::None;
                };
                AnnotationKind::GaussianBlur {
                    region,
                    radius: self.settings.blur_radius,
                }
            }
            ToolMode::Pointer | ToolMode::Pan | ToolMode::Text => return FinishOutcome::None,
        };

        let id = doc.allocate_annotation_id();
        history.apply(EditCommand::AddAnnotation(AnnotationItem { id, kind }), doc);
        FinishOutcome::Committed
    }

    pub fn commit_text(
        &mut self,
        text: &str,
        doc: &mut DocumentState,
        history: &mut EditHistory,
    ) -> bool {
        let trimmed = text.trim();
        let Some(pos) = self.pending_text_pos.take() else {
            return false;
        };
        if trimmed.is_empty() || !pos.x.is_finite() || !pos.y.is_finite() {
            return false;
        }
        let (out_w, out_h) = doc.output_dimensions();
        let id = doc.allocate_annotation_id();
        let item = AnnotationItem {
            id,
            kind: AnnotationKind::Text {
                position: [
                    pos.x.clamp(0.0, out_w as f32),
                    pos.y.clamp(0.0, out_h as f32),
                ],
                content: trimmed.to_owned(),
                color: self.settings.color,
                font_size: self.settings.font_size,
            },
        };
        history.apply(EditCommand::AddAnnotation(item), doc);
        true
    }

    pub fn apply_staged_crop(
        &mut self,
        doc: &mut DocumentState,
        history: &mut EditHistory,
    ) -> bool {
        let Some(output_crop) = self.staged_crop.take() else {
            return false;
        };
        let (base_w, base_h) = doc.base_dimensions();
        let Some(base_crop) =
            doc.transform
                .map_output_rect_to_base_crop(output_crop, base_w, base_h)
        else {
            return false;
        };
        let cmd = EditCommand::SetCrop {
            previous: doc.transform.spec.crop,
            next: Some(base_crop),
            prev_annotations: Arc::from(doc.annotations.as_slice()),
        };
        history.apply(cmd, doc);
        true
    }

    pub fn cancel_active(&mut self) -> bool {
        self.drag.take().is_some()
            || self.staged_crop.take().is_some()
            || self.pending_text_pos.take().is_some()
    }
}

/// Liang-Barsky segment clipping against `[0, width] x [0, height]` so drags completely
/// outside the canvas are rejected rather than flattened onto border edges.
fn clip_segment_to_canvas(p0: Point2, p1: Point2, size: Size2) -> Option<([f32; 2], [f32; 2])> {
    if !p0.x.is_finite() || !p0.y.is_finite() || !p1.x.is_finite() || !p1.y.is_finite() {
        return None;
    }
    if size.width <= 0.0 || size.height <= 0.0 {
        return None;
    }

    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;

    let half_planes = [
        (-dx, p0.x),
        (dx, size.width - p0.x),
        (-dy, p0.y),
        (dy, size.height - p0.y),
    ];

    for (p, q) in half_planes {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                t0 = t0.max(r);
            } else {
                if r < t0 {
                    return None;
                }
                t1 = t1.min(r);
            }
        }
    }

    let start = [
        (p0.x + t0 * dx).clamp(0.0, size.width),
        (p0.y + t0 * dy).clamp(0.0, size.height),
    ];
    let end = [
        (p0.x + t1 * dx).clamp(0.0, size.width),
        (p0.y + t1 * dy).clamp(0.0, size.height),
    ];
    let len = (end[0] - start[0]).hypot(end[1] - start[1]);
    (len >= 1.0).then_some((start, end))
}
