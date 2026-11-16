//! Structured image metadata inspection, policy sanitization, and lossless container updates.

mod container;
mod exif_ifd;
#[cfg(test)]
mod tests;

use std::{io::Cursor, sync::Arc};

use image::ImageDecoder as _;

use super::{Error, color, source::Format, svgz};

pub use container::rewrite_container_metadata;

pub(crate) const MAX_SVG_XML_BYTES: usize = 32 * 1024 * 1024;
const MAX_INSPECT_EDGE_PX: u32 = 65_536;
const MAX_INSPECT_ALLOC_BYTES: u64 = 128 * 1024 * 1024;

/// Read-only technical properties extracted from image container headers and profiles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechnicalFacts {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub file_bytes: Option<u64>,
    pub color_type: String,
    pub has_alpha: bool,
    pub orientation: u8,
    pub icc_profile_name: Option<String>,
    pub has_icc_profile: bool,
    pub has_embedded_thumbnail: bool,
    pub frame_count: Option<u32>,
}

/// Camera and capture telemetry read from EXIF IFD0 / ExifIFD tags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExifSummary {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub exposure_time: Option<String>,
    pub f_number: Option<String>,
    pub iso: Option<u32>,
    pub focal_length: Option<String>,
    pub software: Option<String>,
}

impl ExifSummary {
    /// Returns `true` when no EXIF camera telemetry fields were populated.
    pub fn is_empty(&self) -> bool {
        self.camera_make.is_none()
            && self.camera_model.is_none()
            && self.lens_model.is_none()
            && self.exposure_time.is_none()
            && self.f_number.is_none()
            && self.iso.is_none()
            && self.focal_length.is_none()
            && self.software.is_none()
    }
}

/// Decimal GPS coordinates parsed from or written into the EXIF GPS IFD.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpsCoords {
    pub latitude: f64,
    pub longitude: f64,
}

/// Seven user-editable metadata fields supported by GDS Images without raw tag editing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditableFields {
    pub title: Option<String>,
    pub description: Option<String>,
    pub artist: Option<String>,
    pub copyright: Option<String>,
    pub rating: Option<u8>,
    pub captured_at: Option<String>,
    pub gps: Option<GpsCoords>,
}

impl EditableFields {
    /// Returns `true` when all seven editable metadata fields are unset.
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.artist.is_none()
            && self.copyright.is_none()
            && self.rating.is_none()
            && self.captured_at.is_none()
            && self.gps.is_none()
    }

    /// Trims string fields, drops blank values, and clamps star ratings to `0..=5`.
    pub fn normalized(&self) -> Self {
        Self {
            title: clean_opt(self.title.as_deref()),
            description: clean_opt(self.description.as_deref()),
            artist: clean_opt(self.artist.as_deref()),
            copyright: clean_opt(self.copyright.as_deref()),
            rating: self.rating.map(|value| value.min(5)),
            captured_at: clean_opt(self.captured_at.as_deref()),
            gps: self.gps.filter(|coords| {
                coords.latitude.is_finite()
                    && coords.longitude.is_finite()
                    && (-90.0..=90.0).contains(&coords.latitude)
                    && (-180.0..=180.0).contains(&coords.longitude)
            }),
        }
    }
}

/// Metadata retention policy used during lossless rewrites and pixel exports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SanitizePolicy {
    /// Preserve existing EXIF tags and embedded structures while applying user edits.
    PreserveWithEdits,
    /// Normalize `Orientation` to `1`, drop stale IFD1 thumbnails, and retain safe fields.
    KeepSafeOnPixelExport,
    /// Strip GPS coordinates, author/copyright, serial numbers, MakerNotes, XMP/IPTC/comments,
    /// and thumbnails while keeping color profiles and safe camera exposure facts.
    StripPrivacy,
}

/// Full metadata bundle inspected for the currently loaded image.
#[derive(Clone, Debug, PartialEq)]
pub struct MetadataBundle {
    pub technical: TechnicalFacts,
    pub exif: ExifSummary,
    pub editable: EditableFields,
    pub raw_exif: Option<Vec<u8>>,
    pub raw_icc: Option<Vec<u8>>,
}

/// Parses SVG XML or gzip-compressed `.svgz` bytes with a bounded decompression limit.
///
/// # Errors
/// Returns [`Error::TooLarge`] if raw or decompressed XML exceeds [`MAX_SVG_XML_BYTES`], or
/// [`Error::Svg`] when the gzip wrapper or SVG XML document is invalid.
pub(crate) fn parse_svg_tree(
    bytes: &[u8],
    fontdb: Option<Arc<resvg::usvg::fontdb::Database>>,
) -> Result<resvg::usvg::Tree, Error> {
    let mut options = resvg::usvg::Options::default();
    if let Some(db) = fontdb {
        options.fontdb = db;
    }
    if svgz::is_gzip_magic(bytes) {
        let xml = svgz::decompress_svgz_bounded(bytes, MAX_SVG_XML_BYTES)?;
        return Ok(resvg::usvg::Tree::from_data(&xml, &options)?);
    }
    if bytes.len() > MAX_SVG_XML_BYTES {
        return Err(Error::TooLarge {
            limit_bytes: MAX_SVG_XML_BYTES as u64,
            actual_bytes: bytes.len() as u64,
        });
    }
    Ok(resvg::usvg::Tree::from_data(bytes, &options)?)
}

pub(crate) fn inspect_svg_tree(
    tree: &resvg::usvg::Tree,
    file_bytes: Option<u64>,
) -> MetadataBundle {
    let size = tree.size();
    let width = (size.width().ceil() as u32).max(1);
    let height = (size.height().ceil() as u32).max(1);
    MetadataBundle {
        technical: TechnicalFacts {
            format: Format::Svg,
            width,
            height,
            file_bytes,
            color_type: "SVG".to_owned(),
            has_alpha: true,
            orientation: 1,
            icc_profile_name: None,
            has_icc_profile: false,
            has_embedded_thumbnail: false,
            frame_count: None,
        },
        exif: ExifSummary::default(),
        editable: EditableFields::default(),
        raw_exif: None,
        raw_icc: None,
    }
}

/// Inspects container headers, EXIF IFDs, PNG text chunks, and ICC profiles without full pixel decoding.
///
/// # Errors
/// Returns [`Error::UnsupportedFormat`] when the format cannot be probed, [`Error::Svg`] for
/// invalid SVG XML documents, [`Error::TooLarge`] when an SVG/SVGZ payload exceeds bounds, or
/// [`Error::Decode`] for malformed raster container headers.
pub fn inspect_bytes(
    bytes: &[u8],
    format: Format,
    file_bytes: Option<u64>,
) -> Result<MetadataBundle, Error> {
    if format == Format::Svg {
        let tree = parse_svg_tree(bytes, None)?;
        return Ok(inspect_svg_tree(&tree, file_bytes));
    }

    let image_format = format
        .to_image_format()
        .ok_or_else(|| Error::UnsupportedFormat(format.label().to_owned()))?;
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image_format);
    let mut limits = image::Limits::no_limits();
    limits.max_image_width = Some(MAX_INSPECT_EDGE_PX);
    limits.max_image_height = Some(MAX_INSPECT_EDGE_PX);
    limits.max_alloc = Some(MAX_INSPECT_ALLOC_BYTES);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    let color_type = decoder.color_type();
    let has_alpha = color_type.has_alpha();
    let decoder_orientation = decoder.orientation().ok().map(|o| o.to_exif());
    let raw_icc = decoder
        .icc_profile()
        .ok()
        .flatten()
        .filter(|b| !b.is_empty());
    let raw_exif = decoder
        .exif_metadata()
        .ok()
        .flatten()
        .map(|raw| exif_ifd::strip_exif_header(&raw).to_vec())
        .filter(|b| !b.is_empty());

    let mut parsed = raw_exif
        .as_deref()
        .map(exif_ifd::parse_exif)
        .unwrap_or_default();
    if parsed.orientation <= 1
        && let Some(orient) = decoder_orientation
    {
        parsed.orientation = orient;
    }
    if parsed.orientation == 0 {
        parsed.orientation = 1;
    }
    if format == Format::Png {
        container::inspect_png_text_chunks(bytes, &mut parsed.editable);
    }

    let icc_name = raw_icc.as_deref().and_then(color::icc_profile_name);
    let has_icc = raw_icc.is_some();
    let frame_count = probe_frame_count(bytes, format);

    Ok(MetadataBundle {
        technical: TechnicalFacts {
            format,
            width,
            height,
            file_bytes,
            color_type: format!("{color_type:?}"),
            has_alpha,
            orientation: parsed.orientation,
            icc_profile_name: icc_name,
            has_icc_profile: has_icc,
            has_embedded_thumbnail: parsed.has_embedded_thumbnail,
            frame_count,
        },
        exif: parsed.exif,
        editable: parsed.editable,
        raw_exif,
        raw_icc,
    })
}

fn probe_frame_count(bytes: &[u8], format: Format) -> Option<u32> {
    match format {
        Format::Gif => probe_gif_frames(bytes),
        Format::WebP => probe_webp_frames(bytes),
        Format::Png => probe_apng_frames(bytes),
        _ => None,
    }
}

fn probe_gif_frames(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 13 || (!bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a")) {
        return None;
    }
    let mut pos = 13usize;
    let packed = bytes[10];
    if (packed & 0x80) != 0 {
        let gct_entries = 1usize << usize::from((packed & 0x07) + 1);
        pos = pos.checked_add(gct_entries * 3)?;
    }
    let mut frames = 0u32;
    while let Some(&block) = bytes.get(pos) {
        pos += 1;
        match block {
            0x3B => break,
            0x21 => {
                // Skip extension label then sub-blocks.
                pos = skip_gif_sub_blocks(bytes, pos.checked_add(1)?)?;
            }
            0x2C => {
                frames = frames.saturating_add(1);
                let lct_packed = *bytes.get(pos.checked_add(8)?)?;
                pos = pos.checked_add(9)?;
                if (lct_packed & 0x80) != 0 {
                    let lct_entries = 1usize << usize::from((lct_packed & 0x07) + 1);
                    pos = pos.checked_add(lct_entries * 3)?;
                }
                // Skip 1-byte LZW minimum code size + image data sub-blocks.
                pos = skip_gif_sub_blocks(bytes, pos.checked_add(1)?)?;
            }
            _ => break,
        }
    }
    (frames > 0).then_some(frames)
}

fn skip_gif_sub_blocks(bytes: &[u8], mut pos: usize) -> Option<usize> {
    while let Some(&len) = bytes.get(pos) {
        pos += 1;
        if len == 0 {
            return Some(pos);
        }
        pos = pos.checked_add(usize::from(len))?;
        if pos > bytes.len() {
            return None;
        }
    }
    None
}

fn probe_webp_frames(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    let mut pos = 12usize;
    let mut anmf_frames = 0u32;
    while pos + 8 <= bytes.len() {
        let fourcc = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        if fourcc == b"ANMF" {
            anmf_frames = anmf_frames.saturating_add(1);
        }
        let next = pos
            .checked_add(8)?
            .checked_add(size)?
            .checked_add(size % 2)?;
        if next > bytes.len() {
            break;
        }
        pos = next;
    }
    Some(anmf_frames.max(1))
}

fn probe_apng_frames(bytes: &[u8]) -> Option<u32> {
    const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if !bytes.starts_with(&PNG_SIG) {
        return None;
    }
    let mut pos = 8usize;
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        let kind = &bytes[pos + 4..pos + 8];
        if kind == b"IDAT" || kind == b"IEND" {
            break;
        }
        let data_end = pos.checked_add(8)?.checked_add(len)?;
        if kind == b"acTL" && len == 8 && data_end <= bytes.len() {
            let num_frames = u32::from_be_bytes(bytes[pos + 8..pos + 12].try_into().ok()?);
            return (num_frames > 0).then_some(num_frames);
        }
        pos = data_end.checked_add(4)?;
    }
    None
}

/// Builds a sanitized Little-Endian TIFF payload suitable for `ImageEncoder::set_exif_metadata`.
///
/// Exported pixel buffers have already had EXIF orientation baked in, so `Orientation` is always
/// normalized to `1` and any stale IFD1 embedded thumbnail is stripped.
pub fn build_export_exif(
    existing_exif: Option<&[u8]>,
    edits: Option<&EditableFields>,
    policy: SanitizePolicy,
    pixel_dims: Option<(u32, u32)>,
) -> Vec<u8> {
    exif_ifd::build_exif_tiff(
        existing_exif,
        exif_ifd::TiffBuildOptions {
            policy,
            edits,
            preserve_orientation: false,
            pixel_dims,
        },
    )
}

fn clean_opt(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|trimmed| !trimmed.is_empty())
        .map(str::to_owned)
}
