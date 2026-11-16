//! Straight-RGBA8 encoding with sanitized EXIF embedding and JPEG matte flattening.

use image::{
    ExtendedColorType, ImageEncoder as _, RgbaImage,
    codecs::{
        bmp::BmpEncoder,
        jpeg::JpegEncoder,
        png::{CompressionType, FilterType, PngEncoder},
        qoi::QoiEncoder,
        tiff::TiffEncoder,
        webp::WebPEncoder,
    },
};

use super::{
    Error,
    metadata::{self, EditableFields, SanitizePolicy},
    source::Format,
};

/// Pixel re-encoding options for exports and clipboard serialization.
#[derive(Clone, Debug)]
pub struct EncodeOptions<'a> {
    pub format: Format,
    pub jpeg_quality: u8,
    pub png_fast: bool,
    pub existing_exif: Option<&'a [u8]>,
    pub metadata_edits: Option<&'a EditableFields>,
    pub sanitize_policy: SanitizePolicy,
    pub jpeg_matte: [u8; 3],
}

impl Default for EncodeOptions<'_> {
    fn default() -> Self {
        Self {
            format: Format::Png,
            jpeg_quality: 92,
            png_fast: false,
            existing_exif: None,
            metadata_edits: None,
            sanitize_policy: SanitizePolicy::KeepSafeOnPixelExport,
            jpeg_matte: [255, 255, 255],
        }
    }
}

/// Encodes a composited Straight-RGBA8 buffer according to `options`.
///
/// Decoded images are already normalized to sRGB, so no original wide-gamut ICC profile is
/// re-attached to re-encoded pixel buffers.
///
/// # Errors
/// Returns [`Error::UnsupportedOperation`] for read-only formats or [`Error::Decode`] if the
/// underlying image encoder fails.
pub fn encode_rgba(rgba: &RgbaImage, options: &EncodeOptions<'_>) -> Result<Vec<u8>, Error> {
    if !options.format.capabilities().can_encode {
        return Err(Error::UnsupportedOperation(
            "format does not support image encoding",
        ));
    }

    let width = rgba.width();
    let height = rgba.height();
    let exif_tiff = metadata::build_export_exif(
        options.existing_exif,
        options.metadata_edits,
        options.sanitize_policy,
        Some((width, height)),
    );

    let mut out = Vec::with_capacity((width as usize).saturating_mul(height as usize) + 4096);
    match options.format {
        Format::Png => {
            let compression = if options.png_fast {
                CompressionType::Fast
            } else {
                CompressionType::Default
            };
            let mut encoder =
                PngEncoder::new_with_quality(&mut out, compression, FilterType::Adaptive);
            if !exif_tiff.is_empty() {
                encoder
                    .set_exif_metadata(exif_tiff)
                    .map_err(image::ImageError::Unsupported)?;
            }
            encoder.write_image(rgba.as_raw(), width, height, ExtendedColorType::Rgba8)?;
        }
        Format::Jpeg => {
            let rgb = flatten_alpha_for_jpeg(rgba, options.jpeg_matte);
            let mut encoder =
                JpegEncoder::new_with_quality(&mut out, options.jpeg_quality.clamp(1, 100));
            if !exif_tiff.is_empty() {
                encoder
                    .set_exif_metadata(exif_tiff)
                    .map_err(image::ImageError::Unsupported)?;
            }
            encoder.write_image(&rgb, width, height, ExtendedColorType::Rgb8)?;
        }
        Format::WebP => {
            let mut encoder = WebPEncoder::new_lossless(&mut out);
            if !exif_tiff.is_empty() {
                encoder
                    .set_exif_metadata(exif_tiff)
                    .map_err(image::ImageError::Unsupported)?;
            }
            encoder.write_image(rgba.as_raw(), width, height, ExtendedColorType::Rgba8)?;
        }
        Format::Bmp => {
            BmpEncoder::new(&mut out).write_image(
                rgba.as_raw(),
                width,
                height,
                ExtendedColorType::Rgba8,
            )?;
        }
        Format::Tiff => {
            let mut cursor = std::io::Cursor::new(&mut out);
            TiffEncoder::new(&mut cursor).write_image(
                rgba.as_raw(),
                width,
                height,
                ExtendedColorType::Rgba8,
            )?;
        }
        Format::Qoi => {
            QoiEncoder::new(&mut out).write_image(
                rgba.as_raw(),
                width,
                height,
                ExtendedColorType::Rgba8,
            )?;
        }
        _ => {
            return Err(Error::UnsupportedOperation(
                "format does not support image encoding",
            ));
        }
    }

    Ok(out)
}

fn flatten_alpha_for_jpeg(rgba: &RgbaImage, matte: [u8; 3]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity((rgba.width() as usize) * (rgba.height() as usize) * 3);
    for pixel in rgba.pixels() {
        let [r, g, b, a] = pixel.0;
        if a == 255 {
            rgb.extend_from_slice(&[r, g, b]);
        } else if a == 0 {
            rgb.extend_from_slice(&matte);
        } else {
            let alpha = u16::from(a);
            let inv = 255 - alpha;
            let blend = |fg: u8, bg: u8| -> u8 {
                ((u16::from(fg) * alpha + u16::from(bg) * inv + 127) / 255) as u8
            };
            rgb.extend_from_slice(&[blend(r, matte[0]), blend(g, matte[1]), blend(b, matte[2])]);
        }
    }
    rgb
}
