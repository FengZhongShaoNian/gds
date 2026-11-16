//! Bounds-checked TIFF/EXIF IFD parser and canonical Little-Endian writer.

use std::collections::BTreeMap;

use super::{EditableFields, ExifSummary, GpsCoords, SanitizePolicy};

const TYPE_BYTE: u16 = 1;
const TYPE_ASCII: u16 = 2;
const TYPE_SHORT: u16 = 3;
const TYPE_LONG: u16 = 4;
const TYPE_RATIONAL: u16 = 5;
const TYPE_UNDEFINED: u16 = 7;
const TYPE_IFD: u16 = 13;

// IFD0 tags
const TAG_IMAGE_WIDTH: u16 = 0x0100;
const TAG_IMAGE_LENGTH: u16 = 0x0101;
const TAG_IMAGE_DESCRIPTION: u16 = 0x010E;
const TAG_MAKE: u16 = 0x010F;
const TAG_MODEL: u16 = 0x0110;
const TAG_STRIP_OFFSETS: u16 = 0x0111;
const TAG_ORIENTATION: u16 = 0x0112;
const TAG_STRIP_BYTE_COUNTS: u16 = 0x0117;
const TAG_SOFTWARE: u16 = 0x0131;
const TAG_DATE_TIME: u16 = 0x0132;
const TAG_ARTIST: u16 = 0x013B;
const TAG_HOST_COMPUTER: u16 = 0x013C;
const TAG_TILE_OFFSETS: u16 = 0x0144;
const TAG_TILE_BYTE_COUNTS: u16 = 0x0145;
const TAG_SUB_IFDS: u16 = 0x014A;
const TAG_JPEG_OFFSET: u16 = 0x0201;
const TAG_JPEG_LENGTH: u16 = 0x0202;
const TAG_COPYRIGHT: u16 = 0x8298;
const TAG_EXIF_IFD_POINTER: u16 = 0x8769;
const TAG_GPS_IFD_POINTER: u16 = 0x8825;
const TAG_RATING: u16 = 0x4746;
const TAG_XP_TITLE: u16 = 0x9C9B;
const TAG_XP_COMMENT: u16 = 0x9C9C;
const TAG_XP_AUTHOR: u16 = 0x9C9D;
const TAG_PROCESSING_SOFTWARE: u16 = 0x000B;

// ExifIFD tags
const TAG_EXPOSURE_TIME: u16 = 0x829A;
const TAG_F_NUMBER: u16 = 0x829D;
const TAG_ISO_SPEED: u16 = 0x8827;
const TAG_DATE_TIME_ORIGINAL: u16 = 0x9003;
const TAG_DATE_TIME_DIGITIZED: u16 = 0x9004;
const TAG_FOCAL_LENGTH: u16 = 0x920A;
const TAG_MAKER_NOTE: u16 = 0x927C;
const TAG_USER_COMMENT: u16 = 0x9286;
const TAG_INTEROP_IFD_POINTER: u16 = 0xA005;
const TAG_PIXEL_X_DIM: u16 = 0xA002;
const TAG_PIXEL_Y_DIM: u16 = 0xA003;
const TAG_IMAGE_UNIQUE_ID: u16 = 0xA420;
const TAG_CAMERA_OWNER: u16 = 0xA430;
const TAG_BODY_SERIAL: u16 = 0xA431;
const TAG_LENS_MODEL: u16 = 0xA434;
const TAG_LENS_SERIAL: u16 = 0xA435;
const TAG_CAMERA_SERIAL_NUMBER: u16 = 0xC62F;

// GPS IFD tags
const TAG_GPS_VERSION: u16 = 0x0000;
const TAG_GPS_LAT_REF: u16 = 0x0001;
const TAG_GPS_LAT: u16 = 0x0002;
const TAG_GPS_LON_REF: u16 = 0x0003;
const TAG_GPS_LON: u16 = 0x0004;

const MAX_IFD_ENTRIES: usize = 512;
const MAX_PARSED_TAG_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Endian {
    Little,
    Big,
}

impl Endian {
    fn read_u16(self, data: &[u8]) -> Option<u16> {
        let bytes: [u8; 2] = data.get(..2)?.try_into().ok()?;
        Some(match self {
            Self::Little => u16::from_le_bytes(bytes),
            Self::Big => u16::from_be_bytes(bytes),
        })
    }

    fn read_u32(self, data: &[u8]) -> Option<u32> {
        let bytes: [u8; 4] = data.get(..4)?.try_into().ok()?;
        Some(match self {
            Self::Little => u32::from_le_bytes(bytes),
            Self::Big => u32::from_be_bytes(bytes),
        })
    }
}

#[derive(Clone, Debug)]
struct TagValue {
    field_type: u16,
    count: u32,
    bytes: Vec<u8>,
}

impl TagValue {
    fn ascii(text: &str) -> Self {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        Self {
            field_type: TYPE_ASCII,
            count: bytes.len() as u32,
            bytes,
        }
    }

    fn utf16le_xp(text: &str) -> Self {
        let mut bytes = Vec::with_capacity((text.len() + 1) * 2);
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        Self {
            field_type: TYPE_BYTE,
            count: bytes.len() as u32,
            bytes,
        }
    }

    fn short(value: u16) -> Self {
        Self {
            field_type: TYPE_SHORT,
            count: 1,
            bytes: value.to_le_bytes().to_vec(),
        }
    }

    fn long(value: u32) -> Self {
        Self {
            field_type: TYPE_LONG,
            count: 1,
            bytes: value.to_le_bytes().to_vec(),
        }
    }

    fn rationals(pairs: &[(u32, u32)]) -> Self {
        let mut bytes = Vec::with_capacity(pairs.len() * 8);
        for &(num, den) in pairs {
            bytes.extend_from_slice(&num.to_le_bytes());
            bytes.extend_from_slice(&den.to_le_bytes());
        }
        Self {
            field_type: TYPE_RATIONAL,
            count: pairs.len() as u32,
            bytes,
        }
    }

    fn bytes(field_type: u16, bytes: Vec<u8>) -> Self {
        Self {
            field_type,
            count: bytes.len() as u32,
            bytes,
        }
    }
}

#[derive(Clone, Debug)]
struct ParsedIfd {
    ifd0: BTreeMap<u16, TagValue>,
    exif_ifd: BTreeMap<u16, TagValue>,
    gps_ifd: BTreeMap<u16, TagValue>,
    has_embedded_thumbnail: bool,
    ifd1: Option<(BTreeMap<u16, TagValue>, Vec<u8>)>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ParsedExif {
    pub orientation: u8,
    pub exif: ExifSummary,
    pub editable: EditableFields,
    pub has_embedded_thumbnail: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TiffBuildOptions<'a> {
    pub policy: SanitizePolicy,
    pub edits: Option<&'a EditableFields>,
    pub preserve_orientation: bool,
    pub pixel_dims: Option<(u32, u32)>,
}

pub(super) fn strip_exif_header(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"Exif\0\0").unwrap_or(bytes)
}

pub(super) fn parse_exif(bytes: &[u8]) -> ParsedExif {
    let tiff = strip_exif_header(bytes);
    let Some(parsed) = parse_tiff(tiff) else {
        return ParsedExif {
            orientation: 1,
            ..Default::default()
        };
    };

    let orientation = read_u16_le(&parsed.ifd0, TAG_ORIENTATION)
        .filter(|val| (1..=8).contains(val))
        .unwrap_or(1) as u8;

    let exif = ExifSummary {
        camera_make: read_text(&parsed.ifd0, TAG_MAKE),
        camera_model: read_text(&parsed.ifd0, TAG_MODEL),
        lens_model: read_text(&parsed.exif_ifd, TAG_LENS_MODEL),
        exposure_time: format_exposure(read_rational_le(&parsed.exif_ifd, TAG_EXPOSURE_TIME)),
        f_number: format_f_number(read_rational_le(&parsed.exif_ifd, TAG_F_NUMBER)),
        iso: read_u16_or_u32_le(&parsed.exif_ifd, TAG_ISO_SPEED),
        focal_length: format_focal_length(read_rational_le(&parsed.exif_ifd, TAG_FOCAL_LENGTH)),
        software: read_text(&parsed.ifd0, TAG_SOFTWARE),
    };

    let rating = read_u16_le(&parsed.ifd0, TAG_RATING).map(|val| (val.min(5)) as u8);
    let captured_at = read_text(&parsed.exif_ifd, TAG_DATE_TIME_ORIGINAL)
        .or_else(|| read_text(&parsed.exif_ifd, TAG_DATE_TIME_DIGITIZED))
        .or_else(|| read_text(&parsed.ifd0, TAG_DATE_TIME));

    let editable = EditableFields {
        title: read_xp_utf16le(&parsed.ifd0, TAG_XP_TITLE),
        description: read_text(&parsed.ifd0, TAG_IMAGE_DESCRIPTION),
        artist: read_text(&parsed.ifd0, TAG_ARTIST),
        copyright: read_text(&parsed.ifd0, TAG_COPYRIGHT),
        rating,
        captured_at,
        gps: read_gps(&parsed.gps_ifd),
    };

    ParsedExif {
        orientation,
        exif,
        editable,
        has_embedded_thumbnail: parsed.has_embedded_thumbnail,
    }
}

pub(super) fn build_exif_tiff(existing: Option<&[u8]>, options: TiffBuildOptions<'_>) -> Vec<u8> {
    let mut model = existing
        .map(strip_exif_header)
        .and_then(parse_tiff)
        .unwrap_or(ParsedIfd {
            ifd0: BTreeMap::new(),
            exif_ifd: BTreeMap::new(),
            gps_ifd: BTreeMap::new(),
            has_embedded_thumbnail: false,
            ifd1: None,
        });

    strip_untracked_offsets(&mut model.ifd0, true);
    strip_untracked_offsets(&mut model.exif_ifd, true);
    strip_untracked_offsets(&mut model.gps_ifd, true);
    if let Some((ifd1_map, _)) = &mut model.ifd1 {
        strip_untracked_offsets(ifd1_map, false);
    }

    if let Some(fields) = options.edits {
        apply_editable_fields(&mut model, fields);
    }

    match options.policy {
        SanitizePolicy::PreserveWithEdits => {}
        SanitizePolicy::KeepSafeOnPixelExport => {
            strip_thumbnail(&mut model);
        }
        SanitizePolicy::StripPrivacy => {
            strip_thumbnail(&mut model);
            strip_privacy_tags(&mut model);
        }
    }

    if !options.preserve_orientation || options.policy == SanitizePolicy::KeepSafeOnPixelExport {
        model.ifd0.insert(TAG_ORIENTATION, TagValue::short(1));
    }

    if let Some((width, height)) = options.pixel_dims {
        if model.ifd0.contains_key(&TAG_IMAGE_WIDTH) || model.ifd0.contains_key(&TAG_IMAGE_LENGTH) {
            model.ifd0.insert(TAG_IMAGE_WIDTH, TagValue::long(width));
            model.ifd0.insert(TAG_IMAGE_LENGTH, TagValue::long(height));
        }
        if model.exif_ifd.contains_key(&TAG_PIXEL_X_DIM)
            || model.exif_ifd.contains_key(&TAG_PIXEL_Y_DIM)
        {
            model
                .exif_ifd
                .insert(TAG_PIXEL_X_DIM, TagValue::long(width));
            model
                .exif_ifd
                .insert(TAG_PIXEL_Y_DIM, TagValue::long(height));
        }
    }

    if model.ifd0.is_empty()
        && model.exif_ifd.is_empty()
        && model.gps_ifd.is_empty()
        && model.ifd1.is_none()
    {
        return Vec::new();
    }

    serialize_le_tiff(&model)
}

fn apply_editable_fields(model: &mut ParsedIfd, fields: &EditableFields) {
    set_or_remove(
        &mut model.ifd0,
        TAG_XP_TITLE,
        fields.title.as_deref().map(TagValue::utf16le_xp),
    );
    set_or_remove(
        &mut model.ifd0,
        TAG_IMAGE_DESCRIPTION,
        fields.description.as_deref().map(TagValue::ascii),
    );
    set_or_remove(
        &mut model.ifd0,
        TAG_ARTIST,
        fields.artist.as_deref().map(TagValue::ascii),
    );
    // `ARTIST` is canonical on read; drop stale Windows `XPAuthor` so edits take effect cleanly.
    model.ifd0.remove(&TAG_XP_AUTHOR);
    set_or_remove(
        &mut model.ifd0,
        TAG_COPYRIGHT,
        fields.copyright.as_deref().map(TagValue::ascii),
    );
    set_or_remove(
        &mut model.ifd0,
        TAG_RATING,
        fields.rating.map(|r| TagValue::short(u16::from(r.min(5)))),
    );
    set_or_remove(
        &mut model.exif_ifd,
        TAG_DATE_TIME_ORIGINAL,
        fields.captured_at.as_deref().map(TagValue::ascii),
    );
    if fields.captured_at.is_none() {
        model.exif_ifd.remove(&TAG_DATE_TIME_DIGITIZED);
        model.ifd0.remove(&TAG_DATE_TIME);
    }

    model.gps_ifd.clear();
    if let Some(gps) = fields.gps {
        write_gps_tags(&mut model.gps_ifd, gps);
    }
}

fn strip_untracked_offsets(map: &mut BTreeMap<u16, TagValue>, strip_jpeg_thumb_ptrs: bool) {
    for tag in [
        TAG_EXIF_IFD_POINTER,
        TAG_GPS_IFD_POINTER,
        TAG_INTEROP_IFD_POINTER,
        TAG_SUB_IFDS,
        TAG_STRIP_OFFSETS,
        TAG_STRIP_BYTE_COUNTS,
        TAG_TILE_OFFSETS,
        TAG_TILE_BYTE_COUNTS,
    ] {
        map.remove(&tag);
    }
    if strip_jpeg_thumb_ptrs {
        map.remove(&TAG_JPEG_OFFSET);
        map.remove(&TAG_JPEG_LENGTH);
    }
}

fn set_or_remove(map: &mut BTreeMap<u16, TagValue>, tag: u16, value: Option<TagValue>) {
    if let Some(value) = value {
        map.insert(tag, value);
    } else {
        map.remove(&tag);
    }
}

fn strip_thumbnail(model: &mut ParsedIfd) {
    model.ifd1 = None;
    model.has_embedded_thumbnail = false;
    model.ifd0.remove(&TAG_JPEG_OFFSET);
    model.ifd0.remove(&TAG_JPEG_LENGTH);
}

fn strip_privacy_tags(model: &mut ParsedIfd) {
    model.gps_ifd.clear();
    for tag in [
        TAG_PROCESSING_SOFTWARE,
        TAG_SOFTWARE,
        TAG_ARTIST,
        TAG_HOST_COMPUTER,
        TAG_COPYRIGHT,
        TAG_XP_COMMENT,
        TAG_XP_AUTHOR,
    ] {
        model.ifd0.remove(&tag);
    }
    for tag in [
        TAG_MAKER_NOTE,
        TAG_USER_COMMENT,
        TAG_IMAGE_UNIQUE_ID,
        TAG_CAMERA_OWNER,
        TAG_BODY_SERIAL,
        TAG_LENS_SERIAL,
        TAG_CAMERA_SERIAL_NUMBER,
    ] {
        model.exif_ifd.remove(&tag);
    }
}

fn write_gps_tags(gps_ifd: &mut BTreeMap<u16, TagValue>, coords: GpsCoords) {
    let lat_ref = if coords.latitude >= 0.0 { "N" } else { "S" };
    let lon_ref = if coords.longitude >= 0.0 { "E" } else { "W" };
    gps_ifd.insert(
        TAG_GPS_VERSION,
        TagValue::bytes(TYPE_BYTE, vec![2, 3, 0, 0]),
    );
    gps_ifd.insert(TAG_GPS_LAT_REF, TagValue::ascii(lat_ref));
    gps_ifd.insert(
        TAG_GPS_LAT,
        TagValue::rationals(&decimal_to_dms(coords.latitude)),
    );
    gps_ifd.insert(TAG_GPS_LON_REF, TagValue::ascii(lon_ref));
    gps_ifd.insert(
        TAG_GPS_LON,
        TagValue::rationals(&decimal_to_dms(coords.longitude)),
    );
}

fn decimal_to_dms(coord: f64) -> [(u32, u32); 3] {
    // Round once in ten-thousandths of a second so `59.99999"` carries cleanly into `deg/min`.
    let clamped = if coord.is_finite() {
        coord.abs().min(180.0)
    } else {
        0.0
    };
    let total_ten_thousandths = (clamped * 36_000_000.0).round() as u64;
    let deg = (total_ten_thousandths / 36_000_000) as u32;
    let rem = total_ten_thousandths % 36_000_000;
    let min = (rem / 600_000) as u32;
    let sec_scaled = (rem % 600_000) as u32;
    [(deg, 1), (min, 1), (sec_scaled, 10_000)]
}

fn parse_tiff(tiff: &[u8]) -> Option<ParsedIfd> {
    if tiff.len() < 8 {
        return None;
    }
    let endian = match &tiff[..4] {
        b"II*\0" => Endian::Little,
        b"MM\0*" => Endian::Big,
        _ => return None,
    };
    let mut visited = Vec::with_capacity(4);
    let mut remaining_budget = MAX_PARSED_TAG_BYTES;

    let ifd0_offset = endian.read_u32(&tiff[4..8])? as usize;
    let (ifd0, next_ifd_offset) = read_ifd(
        tiff,
        ifd0_offset,
        endian,
        &mut visited,
        &mut remaining_budget,
    )?;

    let exif_ifd = read_u16_or_u32_le(&ifd0, TAG_EXIF_IFD_POINTER)
        .and_then(|off| {
            read_ifd(
                tiff,
                off as usize,
                endian,
                &mut visited,
                &mut remaining_budget,
            )
        })
        .map(|(map, _)| map)
        .unwrap_or_default();

    let gps_ifd = read_u16_or_u32_le(&ifd0, TAG_GPS_IFD_POINTER)
        .and_then(|off| {
            read_ifd(
                tiff,
                off as usize,
                endian,
                &mut visited,
                &mut remaining_budget,
            )
        })
        .map(|(map, _)| map)
        .unwrap_or_default();

    let ifd1 = if next_ifd_offset > 0 {
        read_ifd(
            tiff,
            next_ifd_offset as usize,
            endian,
            &mut visited,
            &mut remaining_budget,
        )
        .map(|(mut map, _)| {
            let thumb = extract_thumb_bytes(tiff, &map, &mut remaining_budget).unwrap_or_default();
            map.remove(&TAG_JPEG_OFFSET);
            map.remove(&TAG_JPEG_LENGTH);
            (map, thumb)
        })
    } else {
        None
    };

    let has_embedded_thumbnail = ifd1.is_some()
        || ifd0.contains_key(&TAG_JPEG_OFFSET)
        || ifd0.contains_key(&TAG_JPEG_LENGTH);

    Some(ParsedIfd {
        ifd0,
        exif_ifd,
        gps_ifd,
        has_embedded_thumbnail,
        ifd1,
    })
}

fn extract_thumb_bytes(
    tiff: &[u8],
    ifd: &BTreeMap<u16, TagValue>,
    remaining_budget: &mut usize,
) -> Option<Vec<u8>> {
    let offset = read_u16_or_u32_le(ifd, TAG_JPEG_OFFSET)? as usize;
    let len = read_u16_or_u32_le(ifd, TAG_JPEG_LENGTH)? as usize;
    if len > *remaining_budget {
        return None;
    }
    let slice = tiff.get(offset..offset.checked_add(len)?)?;
    *remaining_budget -= slice.len();
    Some(slice.to_vec())
}

fn read_ifd(
    tiff: &[u8],
    offset: usize,
    endian: Endian,
    visited: &mut Vec<usize>,
    remaining_budget: &mut usize,
) -> Option<(BTreeMap<u16, TagValue>, u32)> {
    if offset < 8 || visited.contains(&offset) {
        return None;
    }
    visited.push(offset);

    let count_slice = tiff.get(offset..offset.checked_add(2)?)?;
    let raw_count = usize::from(endian.read_u16(count_slice)?);
    let entries_start = offset + 2;
    let actual_entries_end = entries_start.checked_add(raw_count.checked_mul(12)?)?;
    let count = raw_count.min(MAX_IFD_ENTRIES);
    let clamped_entries_end = entries_start.checked_add(count * 12)?;
    let _ = tiff.get(..clamped_entries_end)?;

    let mut map = BTreeMap::new();
    for idx in 0..count {
        let entry = &tiff[entries_start + idx * 12..entries_start + (idx + 1) * 12];
        let tag = endian.read_u16(&entry[0..2])?;
        let field_type = endian.read_u16(&entry[2..4])?;
        let val_count = endian.read_u32(&entry[4..8])?;
        let Some(unit_size) = type_unit_size(field_type) else {
            continue;
        };
        let Some(byte_len) = (val_count as usize).checked_mul(unit_size) else {
            continue;
        };
        if byte_len > *remaining_budget {
            continue;
        }
        let raw_slice = if byte_len <= 4 {
            &entry[8..8 + byte_len]
        } else {
            let data_offset = endian.read_u32(&entry[8..12])? as usize;
            let Some(end) = data_offset.checked_add(byte_len) else {
                continue;
            };
            let Some(slice) = tiff.get(data_offset..end) else {
                continue;
            };
            slice
        };
        *remaining_budget -= raw_slice.len();
        let normalized = normalize_to_le(raw_slice, field_type, endian);
        map.insert(
            tag,
            TagValue {
                field_type,
                count: val_count,
                bytes: normalized,
            },
        );
    }

    let next_ifd = tiff
        .get(actual_entries_end..actual_entries_end.checked_add(4)?)
        .and_then(|slice| endian.read_u32(slice))
        .unwrap_or(0);
    Some((map, next_ifd))
}

fn type_unit_size(field_type: u16) -> Option<usize> {
    match field_type {
        TYPE_BYTE | TYPE_ASCII | 6 | TYPE_UNDEFINED => Some(1),
        TYPE_SHORT | 8 => Some(2),
        TYPE_LONG | 9 | 11 | TYPE_IFD => Some(4),
        TYPE_RATIONAL | 10 | 12 => Some(8),
        _ => None,
    }
}

fn normalize_to_le(raw: &[u8], field_type: u16, endian: Endian) -> Vec<u8> {
    if endian == Endian::Little {
        return raw.to_vec();
    }
    let mut out = raw.to_vec();
    match field_type {
        TYPE_SHORT | 8 => {
            for chunk in out.as_chunks_mut::<2>().0 {
                chunk.reverse();
            }
        }
        TYPE_LONG | 9 | 11 | TYPE_IFD | TYPE_RATIONAL | 10 => {
            for chunk in out.as_chunks_mut::<4>().0 {
                chunk.reverse();
            }
        }
        12 => {
            for chunk in out.as_chunks_mut::<8>().0 {
                chunk.reverse();
            }
        }
        _ => {}
    }
    out
}

fn read_u16_le(map: &BTreeMap<u16, TagValue>, tag: u16) -> Option<u16> {
    let val = map.get(&tag)?;
    let bytes: [u8; 2] = val.bytes.get(..2)?.try_into().ok()?;
    Some(u16::from_le_bytes(bytes))
}

fn read_u16_or_u32_le(map: &BTreeMap<u16, TagValue>, tag: u16) -> Option<u32> {
    let val = map.get(&tag)?;
    match val.field_type {
        TYPE_SHORT => {
            let bytes: [u8; 2] = val.bytes.get(..2)?.try_into().ok()?;
            Some(u32::from(u16::from_le_bytes(bytes)))
        }
        TYPE_LONG | TYPE_IFD => {
            let bytes: [u8; 4] = val.bytes.get(..4)?.try_into().ok()?;
            Some(u32::from_le_bytes(bytes))
        }
        _ => None,
    }
}

fn read_rational_le(map: &BTreeMap<u16, TagValue>, tag: u16) -> Option<(u32, u32)> {
    read_rational_at(map, tag, 0)
}

fn read_rational_at(map: &BTreeMap<u16, TagValue>, tag: u16, idx: usize) -> Option<(u32, u32)> {
    let val = map.get(&tag)?;
    let base = idx.checked_mul(8)?;
    let num: [u8; 4] = val.bytes.get(base..base + 4)?.try_into().ok()?;
    let den: [u8; 4] = val.bytes.get(base + 4..base + 8)?.try_into().ok()?;
    let den = u32::from_le_bytes(den);
    (den != 0).then_some((u32::from_le_bytes(num), den))
}

fn read_text(map: &BTreeMap<u16, TagValue>, tag: u16) -> Option<String> {
    let val = map.get(&tag)?;
    let trimmed = val.bytes.split(|&b| b == 0).next()?;
    let text = std::str::from_utf8(trimmed).ok()?.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn read_xp_utf16le(map: &BTreeMap<u16, TagValue>, tag: u16) -> Option<String> {
    let val = map.get(&tag)?;
    let mut units = Vec::with_capacity(val.bytes.len() / 2);
    for &pair in val.bytes.as_chunks::<2>().0 {
        let unit = u16::from_le_bytes(pair);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    let text = String::from_utf16(&units).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

fn read_gps(gps_ifd: &BTreeMap<u16, TagValue>) -> Option<GpsCoords> {
    let lat_ref = read_text(gps_ifd, TAG_GPS_LAT_REF)?;
    let lon_ref = read_text(gps_ifd, TAG_GPS_LON_REF)?;
    let lat_sign = match lat_ref.as_str() {
        "N" | "n" => 1.0,
        "S" | "s" => -1.0,
        _ => return None,
    };
    let lon_sign = match lon_ref.as_str() {
        "E" | "e" => 1.0,
        "W" | "w" => -1.0,
        _ => return None,
    };
    let lat = dms_to_decimal(gps_ifd, TAG_GPS_LAT)?;
    let lon = dms_to_decimal(gps_ifd, TAG_GPS_LON)?;
    let latitude = lat * lat_sign;
    let longitude = lon * lon_sign;
    if !latitude.is_finite()
        || !longitude.is_finite()
        || !(-90.0..=90.0).contains(&latitude)
        || !(-180.0..=180.0).contains(&longitude)
    {
        return None;
    }
    Some(GpsCoords {
        latitude,
        longitude,
    })
}

fn dms_to_decimal(gps_ifd: &BTreeMap<u16, TagValue>, tag: u16) -> Option<f64> {
    let (d_n, d_d) = read_rational_at(gps_ifd, tag, 0)?;
    let (m_n, m_d) = read_rational_at(gps_ifd, tag, 1)?;
    let (s_n, s_d) = read_rational_at(gps_ifd, tag, 2)?;
    let deg = f64::from(d_n) / f64::from(d_d);
    let min = f64::from(m_n) / f64::from(m_d);
    let sec = f64::from(s_n) / f64::from(s_d);
    Some(deg + min / 60.0 + sec / 3600.0)
}

fn format_exposure(rational: Option<(u32, u32)>) -> Option<String> {
    let (num, den) = rational?;
    if num == 0 || den == 0 {
        return None;
    }
    if num < den && den.is_multiple_of(num) {
        Some(format!("1/{} s", den / num))
    } else if num < den {
        Some(format!("{num}/{den} s"))
    } else {
        let secs = f64::from(num) / f64::from(den);
        Some(format!("{secs:.1} s"))
    }
}

fn format_f_number(rational: Option<(u32, u32)>) -> Option<String> {
    let (num, den) = rational?;
    if num == 0 || den == 0 {
        return None;
    }
    let val = f64::from(num) / f64::from(den);
    Some(format!("f/{val:.1}"))
}

fn format_focal_length(rational: Option<(u32, u32)>) -> Option<String> {
    let (num, den) = rational?;
    if num == 0 || den == 0 {
        return None;
    }
    let val = f64::from(num) / f64::from(den);
    if (val - val.round()).abs() < 0.05 {
        Some(format!("{} mm", val.round() as u32))
    } else {
        Some(format!("{val:.1} mm"))
    }
}

fn serialize_le_tiff(model: &ParsedIfd) -> Vec<u8> {
    let mut out = Vec::with_capacity(512);
    // TIFF Header: "II*\0" + IFD0 offset (8)
    out.extend_from_slice(b"II*\0");
    out.extend_from_slice(&8u32.to_le_bytes());

    let has_exif = !model.exif_ifd.is_empty();
    let has_gps = !model.gps_ifd.is_empty();

    let mut ifd0 = model.ifd0.clone();
    if has_exif {
        ifd0.insert(TAG_EXIF_IFD_POINTER, TagValue::long(0));
    }
    if has_gps {
        ifd0.insert(TAG_GPS_IFD_POINTER, TagValue::long(0));
    }

    let ifd0_ptr_exif = write_ifd_placeholder(
        &mut out,
        &ifd0,
        has_exif.then_some(TAG_EXIF_IFD_POINTER),
        has_gps.then_some(TAG_GPS_IFD_POINTER),
    );
    let ifd0_next_offset_pos = ifd0_ptr_exif.next_ifd_pos;

    if has_exif {
        let exif_offset = out.len() as u32;
        if let Some(pos) = ifd0_ptr_exif.exif_ptr_pos {
            out[pos..pos + 4].copy_from_slice(&exif_offset.to_le_bytes());
        }
        write_ifd_block(&mut out, &model.exif_ifd, None);
    }

    if has_gps {
        let gps_offset = out.len() as u32;
        if let Some(pos) = ifd0_ptr_exif.gps_ptr_pos {
            out[pos..pos + 4].copy_from_slice(&gps_offset.to_le_bytes());
        }
        write_ifd_block(&mut out, &model.gps_ifd, None);
    }

    if let Some((ifd1_tags, thumb_bytes)) = &model.ifd1
        && !thumb_bytes.is_empty()
    {
        let ifd1_offset = out.len() as u32;
        out[ifd0_next_offset_pos..ifd0_next_offset_pos + 4]
            .copy_from_slice(&ifd1_offset.to_le_bytes());
        let mut tags = ifd1_tags.clone();
        tags.insert(TAG_JPEG_OFFSET, TagValue::long(0));
        tags.insert(TAG_JPEG_LENGTH, TagValue::long(thumb_bytes.len() as u32));
        write_ifd_block(&mut out, &tags, Some(thumb_bytes));
    }

    out
}

struct IfdPatchPositions {
    exif_ptr_pos: Option<usize>,
    gps_ptr_pos: Option<usize>,
    next_ifd_pos: usize,
}

fn write_ifd_placeholder(
    out: &mut Vec<u8>,
    entries: &BTreeMap<u16, TagValue>,
    exif_ptr_tag: Option<u16>,
    gps_ptr_tag: Option<u16>,
) -> IfdPatchPositions {
    let entry_count = entries.len() as u16;
    out.extend_from_slice(&entry_count.to_le_bytes());

    let table_start = out.len();
    out.resize(table_start + entries.len() * 12 + 4, 0);
    let next_ifd_pos = table_start + entries.len() * 12;

    let mut exif_ptr_pos = None;
    let mut gps_ptr_pos = None;

    for (idx, (&tag, val)) in entries.iter().enumerate() {
        let rec = table_start + idx * 12;
        out[rec..rec + 2].copy_from_slice(&tag.to_le_bytes());
        out[rec + 2..rec + 4].copy_from_slice(&val.field_type.to_le_bytes());
        out[rec + 4..rec + 8].copy_from_slice(&val.count.to_le_bytes());

        if Some(tag) == exif_ptr_tag {
            exif_ptr_pos = Some(rec + 8);
            continue;
        }
        if Some(tag) == gps_ptr_tag {
            gps_ptr_pos = Some(rec + 8);
            continue;
        }

        if val.bytes.len() <= 4 {
            out[rec + 8..rec + 8 + val.bytes.len()].copy_from_slice(&val.bytes);
        } else {
            let data_offset = out.len() as u32;
            out[rec + 8..rec + 12].copy_from_slice(&data_offset.to_le_bytes());
            out.extend_from_slice(&val.bytes);
            if val.bytes.len() % 2 == 1 {
                out.push(0);
            }
        }
    }

    IfdPatchPositions {
        exif_ptr_pos,
        gps_ptr_pos,
        next_ifd_pos,
    }
}

fn write_ifd_block(
    out: &mut Vec<u8>,
    entries: &BTreeMap<u16, TagValue>,
    thumb_bytes: Option<&[u8]>,
) {
    let entry_count = entries.len() as u16;
    out.extend_from_slice(&entry_count.to_le_bytes());
    let table_start = out.len();
    out.resize(table_start + entries.len() * 12 + 4, 0);

    let mut thumb_offset_pos = None;
    for (idx, (&tag, val)) in entries.iter().enumerate() {
        let rec = table_start + idx * 12;
        out[rec..rec + 2].copy_from_slice(&tag.to_le_bytes());
        out[rec + 2..rec + 4].copy_from_slice(&val.field_type.to_le_bytes());
        out[rec + 4..rec + 8].copy_from_slice(&val.count.to_le_bytes());

        if tag == TAG_JPEG_OFFSET && thumb_bytes.is_some() {
            thumb_offset_pos = Some(rec + 8);
            continue;
        }

        if val.bytes.len() <= 4 {
            out[rec + 8..rec + 8 + val.bytes.len()].copy_from_slice(&val.bytes);
        } else {
            let data_offset = out.len() as u32;
            out[rec + 8..rec + 12].copy_from_slice(&data_offset.to_le_bytes());
            out.extend_from_slice(&val.bytes);
            if val.bytes.len() % 2 == 1 {
                out.push(0);
            }
        }
    }

    if let (Some(thumb), Some(pos)) = (thumb_bytes, thumb_offset_pos) {
        let offset = out.len() as u32;
        out[pos..pos + 4].copy_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(thumb);
        if thumb.len() % 2 == 1 {
            out.push(0);
        }
    }
}
