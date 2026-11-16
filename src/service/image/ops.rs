//! Offline CPU image transformations, halo-padded redaction operators, and vector/text compositing.

#[cfg(test)]
mod tests;

use std::{borrow::Cow, fmt::Write as _};

use image::{
    Rgba, RgbaImage,
    imageops::{self, FilterType},
};
use resvg::{tiny_skia, usvg};

use super::{Error, PixelRect, system_fonts};

/// Orthogonal rotation steps guaranteed lossless in pixel grid coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OrthogonalRotation {
    #[default]
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

impl OrthogonalRotation {
    pub const fn rotate_cw(self) -> Self {
        match self {
            Self::Deg0 => Self::Deg90,
            Self::Deg90 => Self::Deg180,
            Self::Deg180 => Self::Deg270,
            Self::Deg270 => Self::Deg0,
        }
    }

    pub const fn rotate_ccw(self) -> Self {
        match self {
            Self::Deg0 => Self::Deg270,
            Self::Deg90 => Self::Deg0,
            Self::Deg180 => Self::Deg90,
            Self::Deg270 => Self::Deg180,
        }
    }

    pub const fn swaps_axes(self) -> bool {
        matches!(self, Self::Deg90 | Self::Deg270)
    }

    pub const fn degrees(self) -> u16 {
        match self {
            Self::Deg0 => 0,
            Self::Deg90 => 90,
            Self::Deg180 => 180,
            Self::Deg270 => 270,
        }
    }
}

/// Non-destructive spatial transformation pipeline evaluated before annotation composition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransformSpec {
    pub rotation: OrthogonalRotation,
    pub flip_h: bool,
    pub flip_v: bool,
    pub crop: Option<PixelRect>,
    pub resize: Option<(u32, u32)>,
}

/// Applies document crop, orthogonal rotation/flip, and optional resize in exact order.
///
/// # Errors
/// Returns [`Error::UnsupportedOperation`] if a crop yields zero area or resize specifies `0`.
pub fn apply_transform(source: &RgbaImage, spec: &TransformSpec) -> Result<RgbaImage, Error> {
    let mut current: Cow<'_, RgbaImage> = if let Some(crop) = spec.crop {
        let clamped = crop
            .clamp_to(source.width(), source.height())
            .ok_or(Error::UnsupportedOperation("crop region is empty"))?;
        Cow::Owned(
            imageops::crop_imm(source, clamped.x, clamped.y, clamped.width, clamped.height)
                .to_image(),
        )
    } else {
        Cow::Borrowed(source)
    };

    current = match spec.rotation {
        OrthogonalRotation::Deg0 => current,
        OrthogonalRotation::Deg90 => Cow::Owned(imageops::rotate90(current.as_ref())),
        OrthogonalRotation::Deg180 => match current {
            Cow::Owned(mut owned) => {
                imageops::rotate180_in_place(&mut owned);
                Cow::Owned(owned)
            }
            Cow::Borrowed(borrowed) => Cow::Owned(imageops::rotate180(borrowed)),
        },
        OrthogonalRotation::Deg270 => Cow::Owned(imageops::rotate270(current.as_ref())),
    };

    if spec.flip_h {
        match &mut current {
            Cow::Owned(owned) => imageops::flip_horizontal_in_place(owned),
            Cow::Borrowed(borrowed) => current = Cow::Owned(imageops::flip_horizontal(*borrowed)),
        }
    }
    if spec.flip_v {
        match &mut current {
            Cow::Owned(owned) => imageops::flip_vertical_in_place(owned),
            Cow::Borrowed(borrowed) => current = Cow::Owned(imageops::flip_vertical(*borrowed)),
        }
    }

    if let Some((target_w, target_h)) = spec.resize {
        if target_w == 0 || target_h == 0 {
            return Err(Error::UnsupportedOperation(
                "resize dimensions must be greater than zero",
            ));
        }
        current = Cow::Owned(imageops::resize(
            current.as_ref(),
            target_w,
            target_h,
            FilterType::Lanczos3,
        ));
    }

    Ok(current.into_owned())
}

/// Pixelates `region` into solid `block_size`×`block_size` cells using alpha-weighted averaging.
pub fn apply_mosaic(image: &mut RgbaImage, region: PixelRect, block_size: u32) {
    apply_mosaic_clipped(image, region, block_size, None);
}

fn apply_mosaic_clipped(
    image: &mut RgbaImage,
    region: PixelRect,
    block_size: u32,
    clip: Option<PixelRect>,
) {
    let Some(rect) = region.clamp_to(image.width(), image.height()) else {
        return;
    };
    if let Some(clip_rect) = clip
        && intersect_rect(rect, clip_rect).is_none()
    {
        return;
    }
    let step = block_size.max(2);
    let mut by = rect.y;
    while by < rect.bottom() {
        let bh = (rect.bottom() - by).min(step);
        if let Some(clip_rect) = clip
            && (by + bh <= clip_rect.y || by >= clip_rect.bottom())
        {
            by += bh;
            continue;
        }
        let mut bx = rect.x;
        while bx < rect.right() {
            let bw = (rect.right() - bx).min(step);
            let block = PixelRect::new(bx, by, bw, bh);
            let write_rect = if let Some(clip_rect) = clip {
                let Some(intersection) = intersect_rect(block, clip_rect) else {
                    bx += bw;
                    continue;
                };
                intersection
            } else {
                block
            };
            let avg = average_mosaic_block(image, block);
            fill_mosaic_block(image, write_rect, avg);
            bx += bw;
        }
        by += bh;
    }
}

fn average_mosaic_block(image: &RgbaImage, block: PixelRect) -> Rgba<u8> {
    let mut sum_ra = 0u64;
    let mut sum_ga = 0u64;
    let mut sum_ba = 0u64;
    let mut sum_a = 0u64;
    let mut count = 0u64;
    for py in block.y..block.bottom() {
        for px in block.x..block.right() {
            let [r, g, b, a] = image.get_pixel(px, py).0;
            let a_u64 = u64::from(a);
            sum_ra += u64::from(r) * a_u64;
            sum_ga += u64::from(g) * a_u64;
            sum_ba += u64::from(b) * a_u64;
            sum_a += a_u64;
            count += 1;
        }
    }
    if sum_a == 0 || count == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    let half_a = sum_a / 2;
    let avg_a = (sum_a + count / 2).checked_div(count).unwrap_or(0).min(255) as u8;
    Rgba([
        ((sum_ra + half_a) / sum_a).min(255) as u8,
        ((sum_ga + half_a) / sum_a).min(255) as u8,
        ((sum_ba + half_a) / sum_a).min(255) as u8,
        avg_a,
    ])
}

fn fill_mosaic_block(image: &mut RgbaImage, rect: PixelRect, color: Rgba<u8>) {
    for py in rect.y..rect.bottom() {
        for px in rect.x..rect.right() {
            image.put_pixel(px, py, color);
        }
    }
}

/// Blurs `region` in `O(W * H)` using a 3-pass separable sliding-window Gaussian approximation
/// on premultiplied RGBA8 pixels with halo padding to avoid dark border bleed.
pub fn apply_gaussian_blur(image: &mut RgbaImage, region: PixelRect, radius: u32) {
    let img_w = image.width();
    let img_h = image.height();
    let Some(rect) = region.clamp_to(img_w, img_h) else {
        return;
    };
    let sigma = (radius.max(1) as f32).clamp(1.0, 64.0);
    let widths = kovesi_box_widths(sigma);
    let halo = (widths[0] / 2 + widths[1] / 2 + widths[2] / 2) as u32;
    let Some(padded) = rect.expand_clamped(halo, img_w, img_h) else {
        return;
    };

    let work_w = padded.width as usize;
    let work_h = padded.height as usize;
    let pixel_count = work_w.saturating_mul(work_h);
    let mut buf_a = Vec::with_capacity(pixel_count);
    let src_stride = (img_w as usize).saturating_mul(4);
    let src_raw = image.as_raw();
    let pad_x_byte = (padded.x as usize).saturating_mul(4);
    let row_bytes = work_w.saturating_mul(4);

    for row in 0..work_h {
        let start = (padded.y as usize + row) * src_stride + pad_x_byte;
        for px in src_raw[start..start + row_bytes].as_chunks::<4>().0 {
            let a = u16::from(px[3]);
            buf_a.push([
                u16::from(px[0]) * a,
                u16::from(px[1]) * a,
                u16::from(px[2]) * a,
                a * 255,
            ]);
        }
    }

    let mut buf_b = vec![[0u16; 4]; pixel_count];
    let mut col_sums = vec![[0u32; 4]; work_w];
    for width in widths {
        if width <= 1 {
            continue;
        }
        let half = width / 2;
        box_blur_horizontal(&buf_a, &mut buf_b, work_w, work_h, half, width as u32);
        box_blur_vertical(
            &buf_b,
            &mut buf_a,
            work_w,
            work_h,
            half,
            width as u32,
            &mut col_sums,
        );
    }

    let offset_x = (rect.x - padded.x) as usize;
    let offset_y = (rect.y - padded.y) as usize;
    let dst_raw: &mut [u8] = image.as_mut();
    let dst_x_byte = (rect.x as usize).saturating_mul(4);
    let rect_bytes = (rect.width as usize).saturating_mul(4);

    for row in 0..rect.height as usize {
        let src_start = (offset_y + row) * work_w + offset_x;
        let dst_start = (rect.y as usize + row) * src_stride + dst_x_byte;
        for (dst_px, &[pm_r, pm_g, pm_b, pm_a]) in dst_raw[dst_start..dst_start + rect_bytes]
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(&buf_a[src_start..src_start + rect.width as usize])
        {
            if pm_a == 0 {
                dst_px.copy_from_slice(&[0, 0, 0, 0]);
            } else {
                let a_denom = u32::from(pm_a);
                let half = a_denom / 2;
                dst_px[0] = ((u32::from(pm_r) * 255 + half) / a_denom).min(255) as u8;
                dst_px[1] = ((u32::from(pm_g) * 255 + half) / a_denom).min(255) as u8;
                dst_px[2] = ((u32::from(pm_b) * 255 + half) / a_denom).min(255) as u8;
                dst_px[3] = ((a_denom + 127) / 255).min(255) as u8;
            }
        }
    }
}

fn kovesi_box_widths(sigma: f32) -> [usize; 3] {
    let n = 3.0f32;
    let w_ideal = ((12.0 * sigma * sigma / n) + 1.0).sqrt();
    let mut wl = w_ideal.floor() as u32;
    if wl.is_multiple_of(2) {
        wl = wl.saturating_sub(1);
    }
    wl = wl.max(1);
    let wu = wl + 2;
    let wl_f = wl as f32;
    let m_ideal =
        (12.0 * sigma * sigma - n * wl_f * wl_f - 4.0 * n * wl_f - 3.0 * n) / (-4.0 * wl_f - 4.0);
    let m = (m_ideal.round().clamp(0.0, n)) as usize;
    let mut widths = [wu as usize; 3];
    for slot in widths.iter_mut().take(m) {
        *slot = wl as usize;
    }
    widths
}

fn box_blur_horizontal(
    src: &[[u16; 4]],
    dst: &mut [[u16; 4]],
    width: usize,
    height: usize,
    radius: usize,
    denom: u32,
) {
    let last_x = width.saturating_sub(1) as isize;
    let r = radius as isize;
    let half_denom = denom / 2;

    for row in 0..height {
        let row_start = row * width;
        let src_row = &src[row_start..row_start + width];
        let dst_row = &mut dst[row_start..row_start + width];

        let mut sum = [0u32; 4];
        for offset in -r..=r {
            let px = src_row[offset.clamp(0, last_x) as usize];
            sum[0] += u32::from(px[0]);
            sum[1] += u32::from(px[1]);
            sum[2] += u32::from(px[2]);
            sum[3] += u32::from(px[3]);
        }

        for (col, out) in dst_row.iter_mut().enumerate() {
            *out = [
                ((sum[0] + half_denom) / denom) as u16,
                ((sum[1] + half_denom) / denom) as u16,
                ((sum[2] + half_denom) / denom) as u16,
                ((sum[3] + half_denom) / denom) as u16,
            ];
            if col + 1 < width {
                let col_i = col as isize;
                let old_px = src_row[(col_i - r).clamp(0, last_x) as usize];
                let new_px = src_row[(col_i + r + 1).clamp(0, last_x) as usize];
                sum[0] = sum[0] + u32::from(new_px[0]) - u32::from(old_px[0]);
                sum[1] = sum[1] + u32::from(new_px[1]) - u32::from(old_px[1]);
                sum[2] = sum[2] + u32::from(new_px[2]) - u32::from(old_px[2]);
                sum[3] = sum[3] + u32::from(new_px[3]) - u32::from(old_px[3]);
            }
        }
    }
}

fn box_blur_vertical(
    src: &[[u16; 4]],
    dst: &mut [[u16; 4]],
    width: usize,
    height: usize,
    radius: usize,
    denom: u32,
    col_sums: &mut [[u32; 4]],
) {
    let last_y = height.saturating_sub(1) as isize;
    let r = radius as isize;
    let half_denom = denom / 2;

    col_sums.fill([0; 4]);
    for offset in -r..=r {
        let src_row = &src[(offset.clamp(0, last_y) as usize) * width..];
        for (acc, &px) in col_sums.iter_mut().zip(&src_row[..width]) {
            acc[0] += u32::from(px[0]);
            acc[1] += u32::from(px[1]);
            acc[2] += u32::from(px[2]);
            acc[3] += u32::from(px[3]);
        }
    }

    for row in 0..height {
        let dst_row = &mut dst[row * width..(row + 1) * width];
        for (out, acc) in dst_row.iter_mut().zip(col_sums.iter()) {
            *out = [
                ((acc[0] + half_denom) / denom) as u16,
                ((acc[1] + half_denom) / denom) as u16,
                ((acc[2] + half_denom) / denom) as u16,
                ((acc[3] + half_denom) / denom) as u16,
            ];
        }
        if row + 1 < height {
            let row_i = row as isize;
            let old_start = (row_i - r).clamp(0, last_y) as usize * width;
            let new_start = (row_i + r + 1).clamp(0, last_y) as usize * width;
            let old_row = &src[old_start..old_start + width];
            let new_row = &src[new_start..new_start + width];
            for ((acc, &old_px), &new_px) in col_sums.iter_mut().zip(old_row).zip(new_row) {
                acc[0] = acc[0] + u32::from(new_px[0]) - u32::from(old_px[0]);
                acc[1] = acc[1] + u32::from(new_px[1]) - u32::from(old_px[1]);
                acc[2] = acc[2] + u32::from(new_px[2]) - u32::from(old_px[2]);
                acc[3] = acc[3] + u32::from(new_px[3]) - u32::from(old_px[3]);
            }
        }
    }
}

/// Fills `region` with an opaque color or source-over alpha-blends a translucent RGBA fill.
pub fn apply_solid_fill(image: &mut RgbaImage, region: PixelRect, color: [u8; 4]) {
    let sa = color[3];
    if sa == 0 {
        return;
    }
    let img_w = image.width();
    let Some(rect) = region.clamp_to(img_w, image.height()) else {
        return;
    };
    let stride = (img_w as usize).saturating_mul(4);
    let row_start_byte = (rect.x as usize).saturating_mul(4);
    let rect_bytes = (rect.width as usize).saturating_mul(4);
    let raw: &mut [u8] = image.as_mut();

    if sa == 255 {
        for py in rect.y as usize..rect.bottom() as usize {
            let start = py * stride + row_start_byte;
            for dst_px in raw[start..start + rect_bytes].as_chunks_mut::<4>().0 {
                dst_px.copy_from_slice(&color);
            }
        }
        return;
    }

    let sa_f = f32::from(sa);
    let sa_norm = sa_f / 255.0;
    let sr_pm = f32::from(color[0]) * sa_norm;
    let sg_pm = f32::from(color[1]) * sa_norm;
    let sb_pm = f32::from(color[2]) * sa_norm;

    for py in rect.y as usize..rect.bottom() as usize {
        let start = py * stride + row_start_byte;
        for dst_px in raw[start..start + rect_bytes].as_chunks_mut::<4>().0 {
            blend_premultiplied_over(dst_px, sr_pm, sg_pm, sb_pm, sa_f);
        }
    }
}

fn blend_premultiplied_over(dst: &mut [u8], sr_pm: f32, sg_pm: f32, sb_pm: f32, sa_255: f32) {
    let inv_sa = 255.0 - sa_255;
    let da = f32::from(dst[3]) / 255.0;
    let dr_pm = f32::from(dst[0]) * da;
    let dg_pm = f32::from(dst[1]) * da;
    let db_pm = f32::from(dst[2]) * da;

    let out_a_255 = sa_255 + da * inv_sa;
    let out_a = (out_a_255 / 255.0).max(1e-6);
    let factor = inv_sa / 255.0;

    dst[0] = ((sr_pm + dr_pm * factor) / out_a).round().clamp(0.0, 255.0) as u8;
    dst[1] = ((sg_pm + dg_pm * factor) / out_a).round().clamp(0.0, 255.0) as u8;
    dst[2] = ((sb_pm + db_pm * factor) / out_a).round().clamp(0.0, 255.0) as u8;
    dst[3] = out_a_255.round().clamp(0.0, 255.0) as u8;
}

/// Canvas-space raster/vector drawing operation consumed by offline composition.
#[derive(Clone, Debug, PartialEq)]
pub enum RasterOp {
    Mosaic {
        region: PixelRect,
        block_size: u32,
    },
    GaussianBlur {
        region: PixelRect,
        radius: u32,
    },
    SolidFill {
        region: PixelRect,
        color: [u8; 4],
    },
    Line {
        start: [f32; 2],
        end: [f32; 2],
        color: [u8; 4],
        stroke_width: f32,
    },
    Arrow {
        start: [f32; 2],
        end: [f32; 2],
        color: [u8; 4],
        stroke_width: f32,
    },
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [u8; 4],
        stroke_width: f32,
        fill: Option<[u8; 4]>,
    },
    Ellipse {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [u8; 4],
        stroke_width: f32,
        fill: Option<[u8; 4]>,
    },
    Polyline {
        points: Vec<[f32; 2]>,
        color: [u8; 4],
        stroke_width: f32,
        opacity: f32,
    },
    StepBadge {
        center: [f32; 2],
        number: u32,
        radius: f32,
        fill: [u8; 4],
        text_color: [u8; 4],
    },
    Text {
        position: [f32; 2],
        content: String,
        font_size: f32,
        color: [u8; 4],
    },
}

/// Composites ordered redactions, vector shapes, and typography onto a canvas image.
///
/// # Errors
/// Returns [`Error::Svg`] or [`Error::TooLarge`] if vector overlay rasterization fails.
pub fn composite_ops(canvas: &mut RgbaImage, ops: &[RasterOp]) -> Result<(), Error> {
    composite_ops_clipped(canvas, ops, None)
}

/// Composites ordered redactions, vector shapes, and typography onto a canvas image,
/// optionally restricting writes to `clip`.
///
/// # Errors
/// Returns [`Error::Svg`] or [`Error::TooLarge`] if vector overlay rasterization fails.
pub fn composite_ops_clipped(
    canvas: &mut RgbaImage,
    ops: &[RasterOp],
    clip: Option<PixelRect>,
) -> Result<(), Error> {
    if ops.is_empty() {
        return Ok(());
    }
    let width = canvas.width();
    let height = canvas.height();
    if width == 0 || height == 0 {
        return Ok(());
    }
    let clip_rect = if let Some(raw) = clip {
        let Some(clamped) = raw.clamp_to(width, height) else {
            return Ok(());
        };
        Some(clamped)
    } else {
        None
    };

    let mut svg_body = String::new();
    let mut dirty_bounds: Option<PixelRect> = None;
    for op in ops {
        match op {
            RasterOp::Mosaic { region, block_size } => {
                flush_svg_overlay(canvas, &mut svg_body, &mut dirty_bounds, clip_rect)?;
                apply_mosaic_clipped(canvas, *region, *block_size, clip_rect);
            }
            RasterOp::GaussianBlur { region, radius } => {
                flush_svg_overlay(canvas, &mut svg_body, &mut dirty_bounds, clip_rect)?;
                if let Some(target) = clip_target(*region, clip_rect) {
                    apply_gaussian_blur(canvas, target, *radius);
                }
            }
            RasterOp::SolidFill { region, color } => {
                flush_svg_overlay(canvas, &mut svg_body, &mut dirty_bounds, clip_rect)?;
                if let Some(target) = clip_target(*region, clip_rect) {
                    apply_solid_fill(canvas, target, *color);
                }
            }
            other => {
                if let Some(op_box) = op_dirty_bounds(other)
                    .and_then(|bounds| clamp_f32_bounds(bounds, width, height))
                    .and_then(|rect| clip_target(rect, clip_rect))
                {
                    append_svg_op(&mut svg_body, other);
                    dirty_bounds = Some(match dirty_bounds {
                        Some(existing) => union_rect(existing, op_box),
                        None => op_box,
                    });
                }
            }
        }
    }
    flush_svg_overlay(canvas, &mut svg_body, &mut dirty_bounds, clip_rect)?;
    Ok(())
}

fn clip_target(rect: PixelRect, clip: Option<PixelRect>) -> Option<PixelRect> {
    match clip {
        Some(clip_rect) => intersect_rect(rect, clip_rect),
        None => Some(rect),
    }
}

fn flush_svg_overlay(
    canvas: &mut RgbaImage,
    svg_body: &mut String,
    dirty_bounds: &mut Option<PixelRect>,
    clip_rect: Option<PixelRect>,
) -> Result<(), Error> {
    let Some(raw_rect) = dirty_bounds.take() else {
        svg_body.clear();
        return Ok(());
    };
    let Some(rect) = clip_target(raw_rect, clip_rect) else {
        svg_body.clear();
        return Ok(());
    };
    if svg_body.is_empty() {
        return Ok(());
    }

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\">{svg_body}</svg>",
        rect.width, rect.height, rect.x, rect.y, rect.width, rect.height
    );
    svg_body.clear();

    let options = usvg::Options {
        fontdb: system_fonts(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_data(svg.as_bytes(), &options)?;
    let mut pixmap = tiny_skia::Pixmap::new(rect.width, rect.height).ok_or(Error::TooLarge {
        limit_bytes: u64::from(u32::MAX),
        actual_bytes: u64::from(rect.width)
            .saturating_mul(u64::from(rect.height))
            .saturating_mul(4),
    })?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );

    // Blend premultiplied dirty-region overlay pixmap over Straight RGBA8 destination canvas.
    let canvas_w = canvas.width() as usize;
    let dst: &mut [u8] = canvas.as_mut();
    let pixels = pixmap.pixels();
    let rw = rect.width as usize;

    for row in 0..rect.height as usize {
        let src_row = &pixels[row * rw..(row + 1) * rw];
        let dst_row_start = ((rect.y as usize + row) * canvas_w + rect.x as usize) * 4;
        for (col, src) in src_row.iter().enumerate() {
            let sa = src.alpha();
            if sa == 0 {
                continue;
            }
            let off = dst_row_start + col * 4;
            if sa == 255 {
                dst[off] = src.red();
                dst[off + 1] = src.green();
                dst[off + 2] = src.blue();
                dst[off + 3] = 255;
            } else {
                blend_premultiplied_over(
                    &mut dst[off..off + 4],
                    f32::from(src.red()),
                    f32::from(src.green()),
                    f32::from(src.blue()),
                    f32::from(sa),
                );
            }
        }
    }
    Ok(())
}

fn op_dirty_bounds(op: &RasterOp) -> Option<(f32, f32, f32, f32)> {
    match op {
        RasterOp::Line {
            start,
            end,
            stroke_width,
            ..
        } => {
            if !finite_slice(&[start[0], start[1], end[0], end[1], *stroke_width]) {
                return None;
            }
            let pad = stroke_width.max(1.0) * 0.5 + 2.0;
            Some((
                start[0].min(end[0]) - pad,
                start[1].min(end[1]) - pad,
                start[0].max(end[0]) + pad,
                start[1].max(end[1]) + pad,
            ))
        }
        RasterOp::Arrow {
            start,
            end,
            stroke_width,
            ..
        } => {
            if !finite_slice(&[start[0], start[1], end[0], end[1], *stroke_width]) {
                return None;
            }
            let len = (end[0] - start[0]).hypot(end[1] - start[1]);
            if !len.is_finite() || len < 1e-3 {
                return None;
            }
            let sw = stroke_width.max(1.0);
            let head = (sw * 3.5).clamp(10.0, 32.0).min(len * 0.85);
            let wing = head * 0.55;
            let pad = wing.max(sw * 0.5) + 2.0;
            Some((
                start[0].min(end[0]) - pad,
                start[1].min(end[1]) - pad,
                start[0].max(end[0]) + pad,
                start[1].max(end[1]) + pad,
            ))
        }
        RasterOp::Rect {
            x,
            y,
            width,
            height,
            stroke_width,
            ..
        }
        | RasterOp::Ellipse {
            x,
            y,
            width,
            height,
            stroke_width,
            ..
        } => {
            if !finite_slice(&[*x, *y, *width, *height, *stroke_width]) {
                return None;
            }
            let (nx, ny, nw, nh) = normalize_rect(*x, *y, *width, *height);
            let pad = stroke_width.max(1.0) * 0.5 + 2.0;
            Some((nx - pad, ny - pad, nx + nw + pad, ny + nh + pad))
        }
        RasterOp::Polyline {
            points,
            stroke_width,
            opacity,
            ..
        } => {
            if points.is_empty()
                || !finite_slice(&[*stroke_width, *opacity])
                || points
                    .iter()
                    .any(|p| !p[0].is_finite() || !p[1].is_finite())
            {
                return None;
            }
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for pt in points {
                min_x = min_x.min(pt[0]);
                min_y = min_y.min(pt[1]);
                max_x = max_x.max(pt[0]);
                max_y = max_y.max(pt[1]);
            }
            let pad = stroke_width.max(1.0) * 0.5 + 2.0;
            Some((min_x - pad, min_y - pad, max_x + pad, max_y + pad))
        }
        RasterOp::StepBadge { center, radius, .. } => {
            if !finite_slice(&[center[0], center[1], *radius]) {
                return None;
            }
            let r = radius.max(8.0) + 3.0;
            Some((center[0] - r, center[1] - r, center[0] + r, center[1] + r))
        }
        RasterOp::Text {
            position,
            content,
            font_size,
            ..
        } => {
            let [x, y, w, h] = measure_text_bounds(*position, content, *font_size)?;
            Some((x - 4.0, y - 4.0, x + w + 4.0, y + h + 4.0))
        }
        RasterOp::Mosaic { .. } | RasterOp::GaussianBlur { .. } | RasterOp::SolidFill { .. } => {
            None
        }
    }
}

/// Measures the tight SVG layout bounds `[x, y, width, height]` for [`RasterOp::Text`].
///
/// Uses `usvg::Tree` font layout without allocating a pixel surface, falling back to
/// deterministic proportional metrics when no system fonts are available.
pub fn measure_text_bounds(position: [f32; 2], content: &str, font_size: f32) -> Option<[f32; 4]> {
    if content.trim().is_empty() || !finite_slice(&[position[0], position[1], font_size]) {
        return None;
    }
    let mut body = String::new();
    write_text_svg_elements(&mut body, [0.0, 0.0], content, font_size, [0, 0, 0, 255]);
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8192\" height=\"8192\">{body}</svg>"
    );
    let options = usvg::Options {
        fontdb: system_fonts(),
        ..Default::default()
    };
    if let Ok(tree) = usvg::Tree::from_data(svg.as_bytes(), &options) {
        let bbox = tree.root().bounding_box();
        if !tree.root().children().is_empty() && bbox.width() > 0.5 && bbox.height() > 0.5 {
            return Some([
                position[0] + bbox.left(),
                position[1] + bbox.top(),
                bbox.width(),
                bbox.height(),
            ]);
        }
    }
    Some(fallback_text_bounds(position, content, font_size))
}

fn fallback_text_bounds(position: [f32; 2], content: &str, font_size: f32) -> [f32; 4] {
    let size = font_size.max(8.0);
    let mut line_count = 0usize;
    let mut max_width = 0.0f32;
    for line in content.split('\n') {
        line_count += 1;
        let line_width: f32 = line
            .chars()
            .filter(|&ch| is_xml10_char(ch))
            .map(|ch| match ch {
                'i' | 'l' | '1' | '.' | ',' | ':' | ';' | '!' | '|' | '\'' => 0.32 * size,
                'W' | 'M' | '@' | '%' => 0.84 * size,
                ' ' => 0.30 * size,
                _ if ch.is_ascii() => 0.56 * size,
                _ => 1.0 * size,
            })
            .sum();
        max_width = max_width.max(line_width);
    }
    let width = max_width.max(size * 0.5);
    let top = position[1] + size * 0.15;
    let height = (line_count.max(1) as f32) * size * 1.25;
    [position[0], top, width, height]
}

fn clamp_f32_bounds(
    (min_x, min_y, max_x, max_y): (f32, f32, f32, f32),
    bounds_w: u32,
    bounds_h: u32,
) -> Option<PixelRect> {
    let bw = bounds_w as f32;
    let bh = bounds_h as f32;
    let x0 = min_x.floor().clamp(0.0, bw) as u32;
    let y0 = min_y.floor().clamp(0.0, bh) as u32;
    let x1 = max_x.ceil().clamp(0.0, bw) as u32;
    let y1 = max_y.ceil().clamp(0.0, bh) as u32;
    let width = x1.saturating_sub(x0);
    let height = y1.saturating_sub(y0);
    (width > 0 && height > 0).then(|| PixelRect::new(x0, y0, width, height))
}

fn union_rect(a: PixelRect, b: PixelRect) -> PixelRect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = a.right().max(b.right());
    let y1 = a.bottom().max(b.bottom());
    PixelRect::new(x0, y0, x1 - x0, y1 - y0)
}

fn intersect_rect(a: PixelRect, b: PixelRect) -> Option<PixelRect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = a.right().min(b.right());
    let y1 = a.bottom().min(b.bottom());
    let width = x1.saturating_sub(x0);
    let height = y1.saturating_sub(y0);
    (width > 0 && height > 0).then_some(PixelRect::new(x0, y0, width, height))
}

fn finite_slice(values: &[f32]) -> bool {
    values.iter().all(|v| v.is_finite())
}

fn normalize_rect(x: f32, y: f32, width: f32, height: f32) -> (f32, f32, f32, f32) {
    let (nx, nw) = if width < 0.0 {
        (x + width, -width)
    } else {
        (x, width)
    };
    let (ny, nh) = if height < 0.0 {
        (y + height, -height)
    } else {
        (y, height)
    };
    (nx, ny, nw, nh)
}

fn append_svg_op(out: &mut String, op: &RasterOp) {
    match op {
        RasterOp::Line {
            start,
            end,
            color,
            stroke_width,
        } => {
            if !finite_slice(&[start[0], start[1], end[0], end[1], *stroke_width]) {
                return;
            }
            let _ = write!(
                out,
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-opacity=\"{:.3}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
                start[0],
                start[1],
                end[0],
                end[1],
                svg_hex(*color),
                f32::from(color[3]) / 255.0,
                stroke_width.max(1.0)
            );
        }
        RasterOp::Arrow {
            start,
            end,
            color,
            stroke_width,
        } => {
            if !finite_slice(&[start[0], start[1], end[0], end[1], *stroke_width]) {
                return;
            }
            let sw = stroke_width.max(1.0);
            let dx = end[0] - start[0];
            let dy = end[1] - start[1];
            let len = dx.hypot(dy);
            if !len.is_finite() || len < 1e-3 {
                return;
            }
            let ux = dx / len;
            let uy = dy / len;
            let head = (sw * 3.5).clamp(10.0, 32.0).min(len * 0.85);
            let wing = head * 0.55;
            let bx = end[0] - ux * head;
            let by = end[1] - uy * head;
            let p1x = bx - uy * wing;
            let p1y = by + ux * wing;
            let p2x = bx + uy * wing;
            let p2y = by - ux * wing;
            let line_end_x = end[0] - ux * (head * 0.7);
            let line_end_y = end[1] - uy * (head * 0.7);
            let hex = svg_hex(*color);
            let alpha = f32::from(color[3]) / 255.0;
            let _ = write!(
                out,
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{hex}\" stroke-opacity=\"{alpha:.3}\" stroke-width=\"{sw}\" stroke-linecap=\"round\"/>\
                 <polygon points=\"{},{} {},{} {},{}\" fill=\"{hex}\" fill-opacity=\"{alpha:.3}\"/>",
                start[0], start[1], line_end_x, line_end_y, end[0], end[1], p1x, p1y, p2x, p2y
            );
        }
        RasterOp::Rect {
            x,
            y,
            width,
            height,
            color,
            stroke_width,
            fill,
        } => {
            if !finite_slice(&[*x, *y, *width, *height, *stroke_width]) {
                return;
            }
            let (nx, ny, nw, nh) = normalize_rect(*x, *y, *width, *height);
            let fill_attr = fill
                .map(|c| {
                    format!(
                        "fill=\"{}\" fill-opacity=\"{:.3}\"",
                        svg_hex(c),
                        f32::from(c[3]) / 255.0
                    )
                })
                .unwrap_or_else(|| "fill=\"none\"".to_owned());
            let _ = write!(
                out,
                "<rect x=\"{nx}\" y=\"{ny}\" width=\"{nw}\" height=\"{nh}\" {fill_attr} stroke=\"{}\" stroke-opacity=\"{:.3}\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>",
                svg_hex(*color),
                f32::from(color[3]) / 255.0,
                stroke_width.max(1.0)
            );
        }
        RasterOp::Ellipse {
            x,
            y,
            width,
            height,
            color,
            stroke_width,
            fill,
        } => {
            if !finite_slice(&[*x, *y, *width, *height, *stroke_width]) {
                return;
            }
            let (nx, ny, nw, nh) = normalize_rect(*x, *y, *width, *height);
            let cx = nx + nw * 0.5;
            let cy = ny + nh * 0.5;
            let rx = (nw * 0.5).max(0.5);
            let ry = (nh * 0.5).max(0.5);
            let fill_attr = fill
                .map(|c| {
                    format!(
                        "fill=\"{}\" fill-opacity=\"{:.3}\"",
                        svg_hex(c),
                        f32::from(c[3]) / 255.0
                    )
                })
                .unwrap_or_else(|| "fill=\"none\"".to_owned());
            let _ = write!(
                out,
                "<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\" {fill_attr} stroke=\"{}\" stroke-opacity=\"{:.3}\" stroke-width=\"{}\"/>",
                svg_hex(*color),
                f32::from(color[3]) / 255.0,
                stroke_width.max(1.0)
            );
        }
        RasterOp::Polyline {
            points,
            color,
            stroke_width,
            opacity,
        } => {
            if points.is_empty()
                || !finite_slice(&[*stroke_width, *opacity])
                || points
                    .iter()
                    .any(|p| !p[0].is_finite() || !p[1].is_finite())
            {
                return;
            }
            let alpha = (f32::from(color[3]) / 255.0) * opacity.clamp(0.0, 1.0);
            let _ = write!(out, "<polyline points=\"");
            for (idx, pt) in points.iter().enumerate() {
                if idx > 0 {
                    out.push(' ');
                }
                let _ = write!(out, "{},{}", pt[0], pt[1]);
            }
            let _ = write!(
                out,
                "\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"{alpha:.3}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                svg_hex(*color),
                stroke_width.max(1.0)
            );
        }
        RasterOp::StepBadge {
            center,
            number,
            radius,
            fill,
            text_color,
        } => {
            if !finite_slice(&[center[0], center[1], *radius]) {
                return;
            }
            let r = radius.max(8.0);
            let font_size = r * 1.1;
            let _ = write!(
                out,
                "<circle cx=\"{}\" cy=\"{}\" r=\"{r}\" fill=\"{}\" fill-opacity=\"{:.3}\"/>\
                 <text x=\"{}\" y=\"{}\" text-anchor=\"middle\" dominant-baseline=\"central\" font-size=\"{font_size}\" font-weight=\"700\" font-family=\"sans-serif\" fill=\"{}\" fill-opacity=\"{:.3}\">{number}</text>",
                center[0],
                center[1],
                svg_hex(*fill),
                f32::from(fill[3]) / 255.0,
                center[0],
                center[1],
                svg_hex(*text_color),
                f32::from(text_color[3]) / 255.0
            );
        }
        RasterOp::Text {
            position,
            content,
            font_size,
            color,
        } => {
            write_text_svg_elements(out, *position, content, *font_size, *color);
        }
        RasterOp::Mosaic { .. } | RasterOp::GaussianBlur { .. } | RasterOp::SolidFill { .. } => {}
    }
}

fn write_text_svg_elements(
    out: &mut String,
    position: [f32; 2],
    content: &str,
    font_size: f32,
    color: [u8; 4],
) {
    if content.trim().is_empty() || !finite_slice(&[position[0], position[1], font_size]) {
        return;
    }
    let size = font_size.max(8.0);
    let hex = svg_hex(color);
    let alpha = f32::from(color[3]) / 255.0;
    for (line_idx, line) in content.split('\n').enumerate() {
        let y = position[1] + size + (line_idx as f32) * size * 1.25;
        let escaped = escape_xml(line);
        let _ = write!(
            out,
            "<text x=\"{}\" y=\"{y}\" font-size=\"{size}\" font-family=\"sans-serif\" fill=\"{hex}\" fill-opacity=\"{alpha:.3}\">{escaped}</text>",
            position[0]
        );
    }
}

fn svg_hex(color: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

fn is_xml10_char(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\n' | '\r' | '\u{0020}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
    )
}

fn escape_xml(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        if !is_xml10_char(ch) {
            continue;
        }
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            other => escaped.push(other),
        }
    }
    escaped
}
