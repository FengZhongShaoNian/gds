//! Bounded image decoding, EXIF orientation normalization, ICC -> sRGB CMS, and SVG rendering.

#[cfg(test)]
mod tests;

use std::{
    io::{Cursor, Read as _},
    path::Path,
    sync::Arc,
};

use image::{Limits, metadata::Orientation};
use tokio_util::sync::CancellationToken;

use super::{
    Error, color, io,
    metadata::{self, MetadataBundle},
    source::{FileStamp, Format, Origin},
};

const READ_CHUNK_BYTES: usize = 64 * 1024;

/// Safety limits enforced before allocating pixel buffers during decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeLimits {
    pub max_file_bytes: u64,
    pub max_edge_px: u32,
    pub max_decoded_bytes: u64,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 256 * 1024 * 1024,
            max_edge_px: 32_768,
            max_decoded_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// Decoded RGBA8 image normalized into straight-alpha sRGB pixel coordinates with EXIF
/// orientation already applied.
#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub origin: Origin,
    pub format: Format,
    pub rgba: Arc<image::RgbaImage>,
    pub metadata: MetadataBundle,
    pub was_color_managed: bool,
    pub is_preview_downscaled: bool,
    pub source_bytes: Arc<[u8]>,
}

/// Decodes an [`Origin`] at full resolution under bounded memory/file limits and cancellation.
///
/// # Errors
/// Returns [`Error::Cancelled`], [`Error::TooLarge`], [`Error::DimensionsTooLarge`],
/// [`Error::UnsupportedFormat`], [`Error::Decode`], [`Error::Svg`], or [`Error::Io`].
pub fn decode_origin(
    origin: &Origin,
    limits: DecodeLimits,
    cancel: &CancellationToken,
) -> Result<DecodedImage, Error> {
    decode_internal(origin, None, limits, cancel)
}

/// Decodes an [`Origin`] targeting a bounded `max_edge` preview (direct rasterized SVG scaling or
/// fast raster downscaling when larger than `max_edge`).
///
/// # Errors
/// Returns [`Error::Cancelled`], [`Error::TooLarge`], [`Error::DimensionsTooLarge`],
/// [`Error::UnsupportedFormat`], [`Error::Decode`], [`Error::Svg`], or [`Error::Io`].
pub fn decode_preview(
    origin: &Origin,
    max_edge: u32,
    limits: DecodeLimits,
    cancel: &CancellationToken,
) -> Result<DecodedImage, Error> {
    decode_internal(origin, Some(max_edge.max(1)), limits, cancel)
}

/// Renders an SVG (or gzipped `.svgz`) document at exact target pixel dimensions with straight RGBA8 channels.
///
/// # Errors
/// Returns [`Error::Svg`] for malformed XML/SVG input or [`Error::TooLarge`] when payload bounds or
/// pixmap allocation limits are exceeded.
pub fn render_svg_scaled(bytes: &[u8], width: u32, height: u32) -> Result<image::RgbaImage, Error> {
    let tree = metadata::parse_svg_tree(bytes, Some(super::system_fonts()))?;
    render_svg_tree(&tree, width, height)
}

fn render_svg_tree(
    tree: &resvg::usvg::Tree,
    width: u32,
    height: u32,
) -> Result<image::RgbaImage, Error> {
    use resvg::{tiny_skia, usvg};

    let src = tree.size();
    let src_w = src.width();
    let src_h = src.height();
    if !src_w.is_finite() || !src_h.is_finite() || src_w <= 0.0 || src_h <= 0.0 {
        return Err(Error::Svg(usvg::Error::InvalidSize));
    }

    let out_w = width.max(1);
    let out_h = height.max(1);
    let sx = out_w as f32 / src_w;
    let sy = out_h as f32 / src_h;

    let mut pixmap = tiny_skia::Pixmap::new(out_w, out_h).ok_or(Error::TooLarge {
        limit_bytes: u64::from(u32::MAX),
        actual_bytes: u64::from(out_w)
            .saturating_mul(u64::from(out_h))
            .saturating_mul(4),
    })?;
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );

    demultiply_rgba_in_place(pixmap.data_mut());
    let raw = pixmap.take();

    image::RgbaImage::from_raw(out_w, out_h, raw).ok_or(Error::TooLarge {
        limit_bytes: u64::from(u32::MAX),
        actual_bytes: u64::from(out_w)
            .saturating_mul(u64::from(out_h))
            .saturating_mul(4),
    })
}

pub(crate) fn demultiply_rgba_in_place(data: &mut [u8]) {
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3];
        if a == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
        } else if a < 255 {
            let alpha = u32::from(a);
            let half = alpha / 2;
            px[0] = ((u32::from(px[0]) * 255 + half) / alpha).min(255) as u8;
            px[1] = ((u32::from(px[1]) * 255 + half) / alpha).min(255) as u8;
            px[2] = ((u32::from(px[2]) * 255 + half) / alpha).min(255) as u8;
        }
    }
}

fn decode_internal(
    origin: &Origin,
    preview_edge: Option<u32>,
    limits: DecodeLimits,
    cancel: &CancellationToken,
) -> Result<DecodedImage, Error> {
    check_cancel(cancel)?;
    let (resolved_origin, raw_bytes, format_hint) = load_origin_bytes(origin, limits, cancel)?;
    check_cancel(cancel)?;

    let format = Format::sniff(&raw_bytes, resolved_origin.path())
        .or(format_hint)
        .ok_or_else(|| Error::UnsupportedFormat(resolved_origin.display_name()))?;

    if format == Format::Svg {
        let tree = metadata::parse_svg_tree(&raw_bytes, Some(super::system_fonts()))?;
        check_cancel(cancel)?;
        let metadata = metadata::inspect_svg_tree(&tree, Some(raw_bytes.len() as u64));
        let (src_w, src_h) = (metadata.technical.width, metadata.technical.height);
        check_dimension_and_byte_limits(src_w, src_h, limits)?;

        let (target_w, target_h, downscaled) = match preview_edge {
            Some(edge) if src_w.max(src_h) > edge => {
                let (w, h) = fit_dimensions(src_w, src_h, edge);
                (w, h, true)
            }
            _ => (src_w.max(1), src_h.max(1), false),
        };
        let rgba = render_svg_tree(&tree, target_w, target_h)?;
        return Ok(DecodedImage {
            origin: resolved_origin,
            format,
            rgba: Arc::new(rgba),
            metadata,
            was_color_managed: false,
            is_preview_downscaled: downscaled,
            source_bytes: raw_bytes,
        });
    }

    let mut metadata = metadata::inspect_bytes(&raw_bytes, format, Some(raw_bytes.len() as u64))?;
    check_cancel(cancel)?;
    check_dimension_and_byte_limits(metadata.technical.width, metadata.technical.height, limits)?;

    let image_format = format
        .to_image_format()
        .ok_or_else(|| Error::UnsupportedFormat(format.label().to_owned()))?;
    let mut reader = image::ImageReader::with_format(Cursor::new(&raw_bytes), image_format);
    let mut img_limits = Limits::no_limits();
    img_limits.max_image_width = Some(limits.max_edge_px);
    img_limits.max_image_height = Some(limits.max_edge_px);
    img_limits.max_alloc = Some(limits.max_decoded_bytes);
    reader.limits(img_limits);

    let mut dynamic = reader.decode()?;
    check_cancel(cancel)?;

    let mut is_preview_downscaled = false;
    if let Some(edge) = preview_edge
        && dynamic.width().max(dynamic.height()) > edge
    {
        let (w, h) = fit_dimensions(dynamic.width(), dynamic.height(), edge);
        dynamic = dynamic.resize_exact(w, h, image::imageops::FilterType::Triangle);
        is_preview_downscaled = true;
    }

    if let Some(orient) = Orientation::from_exif(metadata.technical.orientation)
        && orient != Orientation::NoTransforms
    {
        dynamic.apply_orientation(orient);
    }

    let mut rgba = dynamic.into_rgba8();
    check_cancel(cancel)?;

    let was_color_managed = match metadata.raw_icc.as_deref() {
        Some(icc) if !icc.is_empty() => match color::normalize_icc_to_srgb(&mut rgba, icc) {
            Ok(applied) => applied,
            Err(err) => {
                tracing::warn!(error = %err, "ICC profile conversion failed; keeping raw pixels");
                false
            }
        },
        _ => false,
    };

    if is_preview_downscaled {
        if matches!(metadata.technical.orientation, 5..=8) {
            std::mem::swap(
                &mut metadata.technical.width,
                &mut metadata.technical.height,
            );
        }
    } else {
        metadata.technical.width = rgba.width();
        metadata.technical.height = rgba.height();
    }

    Ok(DecodedImage {
        origin: resolved_origin,
        format,
        rgba: Arc::new(rgba),
        metadata,
        was_color_managed,
        is_preview_downscaled,
        source_bytes: raw_bytes,
    })
}

fn check_dimension_and_byte_limits(
    width: u32,
    height: u32,
    limits: DecodeLimits,
) -> Result<(), Error> {
    if width > limits.max_edge_px || height > limits.max_edge_px {
        return Err(Error::DimensionsTooLarge {
            width,
            height,
            max_edge: limits.max_edge_px,
        });
    }
    let rgba_bytes = u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(4);
    if rgba_bytes > limits.max_decoded_bytes {
        return Err(Error::TooLarge {
            limit_bytes: limits.max_decoded_bytes,
            actual_bytes: rgba_bytes,
        });
    }
    Ok(())
}

fn load_origin_bytes(
    origin: &Origin,
    limits: DecodeLimits,
    cancel: &CancellationToken,
) -> Result<(Origin, Arc<[u8]>, Option<Format>), Error> {
    match origin {
        Origin::LocalFile { path, .. } => {
            let (bytes, stamp) = read_bounded_file(path, limits.max_file_bytes, cancel)?;
            let resolved = Origin::LocalFile {
                path: path.clone(),
                stamp,
            };
            Ok((resolved, bytes, Format::from_path(path)))
        }
        Origin::Memory {
            bytes, format_hint, ..
        } => {
            let actual = bytes.len() as u64;
            if actual > limits.max_file_bytes {
                return Err(Error::TooLarge {
                    limit_bytes: limits.max_file_bytes,
                    actual_bytes: actual,
                });
            }
            Ok((origin.clone(), Arc::clone(bytes), *format_hint))
        }
    }
}

fn read_bounded_file(
    path: &Path,
    max_file_bytes: u64,
    cancel: &CancellationToken,
) -> Result<(Arc<[u8]>, Option<FileStamp>), Error> {
    let (mut file, meta, stamp) = io::open_for_read(path)?;
    if meta.len() > max_file_bytes {
        return Err(Error::TooLarge {
            limit_bytes: max_file_bytes,
            actual_bytes: meta.len(),
        });
    }

    let capacity = usize::try_from(meta.len()).unwrap_or(READ_CHUNK_BYTES);
    let mut out = Vec::with_capacity(capacity);
    let mut chunk = [0u8; READ_CHUNK_BYTES];
    loop {
        check_cancel(cancel)?;
        let read = file
            .read(&mut chunk)
            .map_err(|source| Error::io(Some(path.into()), source))?;
        if read == 0 {
            break;
        }
        let next_len = (out.len() as u64).saturating_add(read as u64);
        if next_len > max_file_bytes {
            return Err(Error::TooLarge {
                limit_bytes: max_file_bytes,
                actual_bytes: next_len,
            });
        }
        out.extend_from_slice(&chunk[..read]);
    }
    Ok((Arc::from(out), stamp))
}

fn fit_dimensions(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    let long = width.max(height).max(1);
    if long <= max_edge {
        return (width.max(1), height.max(1));
    }
    let scale = f64::from(max_edge) / f64::from(long);
    let w = (f64::from(width) * scale).round().max(1.0) as u32;
    let h = (f64::from(height) * scale).round().max(1.0) as u32;
    (w, h)
}

fn check_cancel(cancel: &CancellationToken) -> Result<(), Error> {
    if cancel.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
