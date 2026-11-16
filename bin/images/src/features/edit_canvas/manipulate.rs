//! Pure geometry manipulation: rigid group clamping, handle resizing, 15-degree snap, and reordering.

use crate::{
    entities::document::{AnnotationId, AnnotationItem, AnnotationKind, measure_text_rect},
    shared::geometry::{Point2, Rect2},
};

use super::{hit_test::HandleKind, selection::SelectionSet};

const MIN_BOX_SIZE: f32 = 2.0;
const MIN_LINE_LEN: f32 = 2.0;
const MIN_TEXT_SIZE: f32 = 8.0;
const MAX_TEXT_SIZE: f32 = 160.0;
const DUPLICATE_OFFSET: f32 = 16.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResizeModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReorderOp {
    BringForward,
    SendBackward,
    BringToFront,
    SendToBack,
}

/// Translates `originals` together by one shared displacement clamped to `(canvas_w, canvas_h)`
/// so relative member spacing never squashes when any member reaches a canvas edge.
pub fn translate_group_clamped(
    originals: &[AnnotationItem],
    raw_dx: f32,
    raw_dy: f32,
    canvas_size: (u32, u32),
    shift_axis_lock: bool,
) -> (Vec<AnnotationItem>, f32, f32) {
    if originals.is_empty() {
        return (Vec::new(), 0.0, 0.0);
    }
    let (dx, dy) = if shift_axis_lock {
        if raw_dx.abs() >= raw_dy.abs() {
            (raw_dx, 0.0)
        } else {
            (0.0, raw_dy)
        }
    } else {
        (raw_dx, raw_dy)
    };

    let Some(union) = union_of(originals) else {
        return (originals.to_vec(), 0.0, 0.0);
    };
    let (cw, ch) = canvas_size;
    let wf = cw as f32;
    let hf = ch as f32;

    // If an item already extends outside the canvas (from a prior crop), keep `0.0` accessible
    // so nudging inward does not snap violently across the edge.
    let min_dx = (-union.x).min(0.0);
    let max_dx = (wf - union.right()).max(0.0);
    let min_dy = (-union.y).min(0.0);
    let max_dy = (hf - union.bottom()).max(0.0);

    let clamped_dx = dx.clamp(min_dx, max_dx);
    let clamped_dy = dy.clamp(min_dy, max_dy);

    let moved = originals
        .iter()
        .map(|item| item.translated(clamped_dx, clamped_dy))
        .collect();
    (moved, clamped_dx, clamped_dy)
}

pub fn compute_duplicate_offset(
    originals: &[AnnotationItem],
    canvas_w: u32,
    canvas_h: u32,
) -> (f32, f32) {
    let Some(union) = union_of(originals) else {
        return (0.0, 0.0);
    };
    let wf = canvas_w as f32;
    let hf = canvas_h as f32;
    let dx = axis_duplicate_offset(union.x, union.right(), wf);
    let dy = axis_duplicate_offset(union.y, union.bottom(), hf);
    (dx, dy)
}

fn axis_duplicate_offset(min: f32, max: f32, limit: f32) -> f32 {
    if max + DUPLICATE_OFFSET <= limit {
        DUPLICATE_OFFSET
    } else if min - DUPLICATE_OFFSET >= 0.0 {
        -DUPLICATE_OFFSET
    } else if limit - max >= 1.0 {
        limit - max
    } else if min >= 1.0 {
        -min
    } else {
        0.0
    }
}

pub fn resize_item_with_handle(
    original: &AnnotationItem,
    handle: HandleKind,
    target_img_pt: Point2,
    canvas_size: (u32, u32),
    modifiers: ResizeModifiers,
) -> AnnotationItem {
    let wf = (canvas_size.0 as f32).max(MIN_BOX_SIZE);
    let hf = (canvas_size.1 as f32).max(MIN_BOX_SIZE);
    let target = Point2::new(
        target_img_pt.x.clamp(0.0, wf),
        target_img_pt.y.clamp(0.0, hf),
    );

    let kind = match &original.kind {
        AnnotationKind::Rect {
            rect,
            stroke,
            stroke_width,
            fill,
        } => AnnotationKind::Rect {
            rect: resize_box(*rect, handle, target, (wf, hf), modifiers),
            stroke: *stroke,
            stroke_width: *stroke_width,
            fill: *fill,
        },
        AnnotationKind::Ellipse {
            rect,
            stroke,
            stroke_width,
            fill,
        } => AnnotationKind::Ellipse {
            rect: resize_box(*rect, handle, target, (wf, hf), modifiers),
            stroke: *stroke,
            stroke_width: *stroke_width,
            fill: *fill,
        },
        AnnotationKind::Mosaic { region, block_size } => AnnotationKind::Mosaic {
            region: resize_box(*region, handle, target, (wf, hf), modifiers),
            block_size: *block_size,
        },
        AnnotationKind::GaussianBlur { region, radius } => AnnotationKind::GaussianBlur {
            region: resize_box(*region, handle, target, (wf, hf), modifiers),
            radius: *radius,
        },
        AnnotationKind::Arrow {
            start,
            end,
            color,
            stroke_width,
        } => {
            let (next_start, next_end) =
                resize_segment(*start, *end, handle, target, (wf, hf), modifiers.shift);
            AnnotationKind::Arrow {
                start: next_start,
                end: next_end,
                color: *color,
                stroke_width: *stroke_width,
            }
        }
        AnnotationKind::Line {
            start,
            end,
            color,
            stroke_width,
        } => {
            let (next_start, next_end) =
                resize_segment(*start, *end, handle, target, (wf, hf), modifiers.shift);
            AnnotationKind::Line {
                start: next_start,
                end: next_end,
                color: *color,
                stroke_width: *stroke_width,
            }
        }
        AnnotationKind::Text {
            position,
            content,
            color,
            font_size,
        } => {
            let (next_position, next_font_size) =
                resize_text(*position, content, *font_size, handle, target, (wf, hf));
            AnnotationKind::Text {
                position: next_position,
                content: content.clone(),
                color: *color,
                font_size: next_font_size,
            }
        }
    };

    AnnotationItem {
        id: original.id,
        kind,
    }
}

pub fn snap_angle_15(fixed: Point2, moving: Point2, wf: f32, hf: f32) -> Point2 {
    let dx = moving.x - fixed.x;
    let dy = moving.y - fixed.y;
    let raw_len = dx.hypot(dy);
    if raw_len < 1e-3 {
        return moving;
    }
    let step = std::f32::consts::PI / 12.0;
    let angle = (dy.atan2(dx) / step).round() * step;
    let (uy, ux) = angle.sin_cos();
    let proj_len = (dx * ux + dy * uy).max(0.0);

    let max_x = if ux > 1e-6 {
        (wf - fixed.x) / ux
    } else if ux < -1e-6 {
        -fixed.x / ux
    } else {
        f32::INFINITY
    };
    let max_y = if uy > 1e-6 {
        (hf - fixed.y) / uy
    } else if uy < -1e-6 {
        -fixed.y / uy
    } else {
        f32::INFINITY
    };
    let clamped_len = proj_len.min(max_x.min(max_y).max(0.0));
    Point2::new(
        (fixed.x + ux * clamped_len).clamp(0.0, wf),
        (fixed.y + uy * clamped_len).clamp(0.0, hf),
    )
}

pub fn reorder_annotations(
    annotations: &[AnnotationItem],
    selection: &SelectionSet,
    op: ReorderOp,
) -> Option<Vec<AnnotationId>> {
    if selection.is_empty() || annotations.len() < 2 {
        return None;
    }
    let original: Vec<AnnotationId> = annotations.iter().map(|item| item.id).collect();
    let mut order = original.clone();

    match op {
        ReorderOp::BringForward => {
            for i in (0..order.len().saturating_sub(1)).rev() {
                if selection.contains(order[i]) && !selection.contains(order[i + 1]) {
                    order.swap(i, i + 1);
                }
            }
        }
        ReorderOp::SendBackward => {
            for i in 1..order.len() {
                if selection.contains(order[i]) && !selection.contains(order[i - 1]) {
                    order.swap(i, i - 1);
                }
            }
        }
        ReorderOp::BringToFront => {
            let mut next = Vec::with_capacity(order.len());
            for &id in &order {
                if !selection.contains(id) {
                    next.push(id);
                }
            }
            for &id in &order {
                if selection.contains(id) {
                    next.push(id);
                }
            }
            order = next;
        }
        ReorderOp::SendToBack => {
            let mut next = Vec::with_capacity(order.len());
            for &id in &order {
                if selection.contains(id) {
                    next.push(id);
                }
            }
            for &id in &order {
                if !selection.contains(id) {
                    next.push(id);
                }
            }
            order = next;
        }
    }

    (order != original).then_some(order)
}

fn union_of(items: &[AnnotationItem]) -> Option<Rect2> {
    let mut union: Option<Rect2> = None;
    for item in items {
        let b = item.visual_bounds();
        union = Some(union.map_or(b, |acc| acc.union(b)));
    }
    union
}

fn resize_box(
    rect: Rect2,
    handle: HandleKind,
    target: Point2,
    canvas: (f32, f32),
    modifiers: ResizeModifiers,
) -> Rect2 {
    let orig = rect.normalized();
    let (wf, hf) = canvas;

    if modifiers.alt {
        return resize_box_centered(orig, handle, target, (wf, hf), modifiers);
    }

    let anchor_x = match handle {
        HandleKind::Nw | HandleKind::W | HandleKind::Sw => Some(orig.right()),
        HandleKind::Ne | HandleKind::E | HandleKind::Se => Some(orig.x),
        _ => None,
    };
    let anchor_y = match handle {
        HandleKind::Nw | HandleKind::N | HandleKind::Ne => Some(orig.bottom()),
        HandleKind::Sw | HandleKind::S | HandleKind::Se => Some(orig.y),
        _ => None,
    };

    if let (Some(ax), Some(ay)) = (anchor_x, anchor_y) {
        let raw_dx = target.x - ax;
        let raw_dy = target.y - ay;
        let sx = if raw_dx >= 0.0 { 1.0 } else { -1.0 };
        let sy = if raw_dy >= 0.0 { 1.0 } else { -1.0 };
        let max_w = if sx > 0.0 { wf - ax } else { ax }.max(0.0);
        let max_h = if sy > 0.0 { hf - ay } else { ay }.max(0.0);

        let (w, h) = if modifiers.shift {
            let max_side = max_w.min(max_h);
            let desired = raw_dx.abs().max(raw_dy.abs());
            let side = desired.clamp(MIN_BOX_SIZE.min(max_side), max_side);
            (side, side)
        } else if modifiers.ctrl && orig.width > 0.0 && orig.height > 0.0 {
            let max_scale = (max_w / orig.width).min(max_h / orig.height);
            let min_scale = (MIN_BOX_SIZE / orig.width).max(MIN_BOX_SIZE / orig.height);
            let raw_scale = (raw_dx.abs() / orig.width).max(raw_dy.abs() / orig.height);
            let scale = raw_scale.clamp(min_scale.min(max_scale), max_scale);
            (orig.width * scale, orig.height * scale)
        } else {
            (
                raw_dx.abs().clamp(MIN_BOX_SIZE.min(max_w), max_w),
                raw_dy.abs().clamp(MIN_BOX_SIZE.min(max_h), max_h),
            )
        };

        let x1 = (ax + sx * w).clamp(0.0, wf);
        let y1 = (ay + sy * h).clamp(0.0, hf);
        return Rect2::new(ax.min(x1), ay.min(y1), (x1 - ax).abs(), (y1 - ay).abs());
    }

    let (x0, x1) = if let Some(ax) = anchor_x {
        let raw_dx = target.x - ax;
        let sx = if raw_dx >= 0.0 { 1.0 } else { -1.0 };
        let max_w = if sx > 0.0 { wf - ax } else { ax }.max(0.0);
        let w = raw_dx.abs().clamp(MIN_BOX_SIZE.min(max_w), max_w);
        let nx = (ax + sx * w).clamp(0.0, wf);
        (ax.min(nx), ax.max(nx))
    } else {
        (orig.x.clamp(0.0, wf), orig.right().clamp(0.0, wf))
    };

    let (y0, y1) = if let Some(ay) = anchor_y {
        let raw_dy = target.y - ay;
        let sy = if raw_dy >= 0.0 { 1.0 } else { -1.0 };
        let max_h = if sy > 0.0 { hf - ay } else { ay }.max(0.0);
        let h = raw_dy.abs().clamp(MIN_BOX_SIZE.min(max_h), max_h);
        let ny = (ay + sy * h).clamp(0.0, hf);
        (ay.min(ny), ay.max(ny))
    } else {
        (orig.y.clamp(0.0, hf), orig.bottom().clamp(0.0, hf))
    };

    Rect2::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

fn resize_box_centered(
    orig: Rect2,
    handle: HandleKind,
    target: Point2,
    canvas: (f32, f32),
    modifiers: ResizeModifiers,
) -> Rect2 {
    let (wf, hf) = canvas;
    let center = orig.center();
    let cx = center.x.clamp(0.0, wf);
    let cy = center.y.clamp(0.0, hf);
    let max_hw = cx.min(wf - cx).max(0.0);
    let max_hh = cy.min(hf - cy).max(0.0);
    let min_half = MIN_BOX_SIZE * 0.5;

    let moves_x = !matches!(handle, HandleKind::N | HandleKind::S);
    let moves_y = !matches!(handle, HandleKind::E | HandleKind::W);
    let req_hw = if moves_x {
        (target.x - cx).abs()
    } else {
        orig.width * 0.5
    };
    let req_hh = if moves_y {
        (target.y - cy).abs()
    } else {
        orig.height * 0.5
    };

    let (hw, hh) = if moves_x && moves_y && modifiers.shift {
        let max_side = max_hw.min(max_hh);
        let s = req_hw.max(req_hh).clamp(min_half.min(max_side), max_side);
        (s, s)
    } else if moves_x && moves_y && modifiers.ctrl && orig.width > 0.0 && orig.height > 0.0 {
        let orig_hw = orig.width * 0.5;
        let orig_hh = orig.height * 0.5;
        let max_scale = (max_hw / orig_hw).min(max_hh / orig_hh);
        let min_scale = (min_half / orig_hw).max(min_half / orig_hh);
        let scale = (req_hw / orig_hw)
            .max(req_hh / orig_hh)
            .clamp(min_scale.min(max_scale), max_scale);
        (orig_hw * scale, orig_hh * scale)
    } else {
        (
            req_hw.clamp(min_half.min(max_hw), max_hw),
            req_hh.clamp(min_half.min(max_hh), max_hh),
        )
    };

    Rect2::new(cx - hw, cy - hh, hw * 2.0, hh * 2.0)
}

fn resize_segment(
    start: [f32; 2],
    end: [f32; 2],
    handle: HandleKind,
    target: Point2,
    canvas: (f32, f32),
    shift_snap: bool,
) -> ([f32; 2], [f32; 2]) {
    let (wf, hf) = canvas;
    let (fixed, moving_start) = match handle {
        HandleKind::LineStart => (Point2::new(end[0], end[1]), Point2::new(start[0], start[1])),
        _ => (Point2::new(start[0], start[1]), Point2::new(end[0], end[1])),
    };
    let next = if shift_snap {
        snap_angle_15(fixed, target, wf, hf)
    } else {
        target
    };
    let len = (next.x - fixed.x).hypot(next.y - fixed.y);
    let final_pt = if len < MIN_LINE_LEN {
        moving_start
    } else {
        next
    };
    match handle {
        HandleKind::LineStart => ([final_pt.x, final_pt.y], end),
        _ => (start, [final_pt.x, final_pt.y]),
    }
}

fn resize_text(
    orig_pos: [f32; 2],
    content: &str,
    orig_size: f32,
    handle: HandleKind,
    target: Point2,
    canvas: (f32, f32),
) -> ([f32; 2], f32) {
    let (wf, hf) = canvas;
    let orig_box = measure_text_rect(orig_pos, content, orig_size);
    let orig_w = orig_box.width.max(1.0);
    let orig_h = orig_box.height.max(1.0);

    let (anchor, req_w, req_h, avail_w, avail_h) = match handle {
        HandleKind::Nw => {
            let anchor = Point2::new(orig_box.right(), orig_box.bottom());
            (
                anchor,
                anchor.x - target.x,
                anchor.y - target.y,
                anchor.x.max(1.0),
                anchor.y.max(1.0),
            )
        }
        HandleKind::Ne => {
            let anchor = Point2::new(orig_box.x, orig_box.bottom());
            (
                anchor,
                target.x - anchor.x,
                anchor.y - target.y,
                (wf - anchor.x).max(1.0),
                anchor.y.max(1.0),
            )
        }
        HandleKind::Sw => {
            let anchor = Point2::new(orig_box.right(), orig_box.y);
            (
                anchor,
                anchor.x - target.x,
                target.y - anchor.y,
                anchor.x.max(1.0),
                (hf - anchor.y).max(1.0),
            )
        }
        _ => {
            let anchor = Point2::new(orig_box.x, orig_box.y);
            (
                anchor,
                target.x - anchor.x,
                target.y - anchor.y,
                (wf - anchor.x).max(1.0),
                (hf - anchor.y).max(1.0),
            )
        }
    };

    let scale = ((req_w * orig_w + req_h * orig_h) / (orig_w * orig_w + orig_h * orig_h)).max(0.05);
    let mut next_size = (orig_size * scale).clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE);

    // Cap `next_size` via binary search if growing would push the text outside the canvas while
    // holding the opposite corner stationary.
    let fits = |sz: f32| {
        let b = measure_text_rect([0.0, 0.0], content, sz);
        b.width <= avail_w && b.height <= avail_h
    };
    if !fits(next_size) && fits(MIN_TEXT_SIZE) {
        let mut lo = MIN_TEXT_SIZE;
        let mut hi = next_size;
        for _ in 0..10 {
            let mid = (lo + hi) * 0.5;
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        next_size = lo;
    }

    let rel = measure_text_rect([0.0, 0.0], content, next_size);
    let next_pos = match handle {
        HandleKind::Nw => [
            anchor.x - (rel.x + rel.width),
            anchor.y - (rel.y + rel.height),
        ],
        HandleKind::Ne => [anchor.x - rel.x, anchor.y - (rel.y + rel.height)],
        HandleKind::Sw => [anchor.x - (rel.x + rel.width), anchor.y - rel.y],
        _ => [anchor.x - rel.x, anchor.y - rel.y],
    };
    (next_pos, next_size)
}
