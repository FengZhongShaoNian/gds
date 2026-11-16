use std::io::Cursor;

use image::{ImageEncoder as _, Rgba, RgbaImage};

use super::{
    EditableFields, GpsCoords, SanitizePolicy, build_export_exif, exif_ifd, inspect_bytes,
    rewrite_container_metadata,
};
use crate::service::image::source::Format;

fn sample_rgba_pattern() -> RgbaImage {
    let mut img = RgbaImage::new(8, 6);
    img.put_pixel(0, 0, Rgba([12, 34, 56, 255]));
    img.put_pixel(7, 5, Rgba([210, 180, 90, 255]));
    img.put_pixel(3, 2, Rgba([40, 200, 120, 255]));
    img
}

fn encode_sample(format: Format) -> Vec<u8> {
    let rgba = sample_rgba_pattern();
    let mut out = Vec::new();
    match format {
        Format::Jpeg => {
            let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95);
            encoder
                .set_icc_profile(sample_srgb_icc())
                .expect("set jpeg icc profile");
            encoder
                .write_image(rgb.as_raw(), 8, 6, image::ExtendedColorType::Rgb8)
                .expect("encode jpeg");
        }
        Format::Png => {
            let mut encoder = image::codecs::png::PngEncoder::new(&mut out);
            encoder
                .set_icc_profile(sample_srgb_icc())
                .expect("set png icc profile");
            encoder
                .write_image(rgba.as_raw(), 8, 6, image::ExtendedColorType::Rgba8)
                .expect("encode png");
        }
        Format::WebP => {
            let mut encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut out);
            encoder
                .set_icc_profile(sample_srgb_icc())
                .expect("set webp icc profile");
            encoder
                .write_image(rgba.as_raw(), 8, 6, image::ExtendedColorType::Rgba8)
                .expect("encode webp");
        }
        _ => panic!("unexpected format for container test"),
    }
    out
}

fn sample_srgb_icc() -> Vec<u8> {
    moxcms::ColorProfile::new_srgb()
        .encode()
        .expect("encode built-in sRGB ICC profile")
}

fn decode_rgba(bytes: &[u8], format: Format) -> Vec<u8> {
    let image_format = format.to_image_format().expect("raster image format");
    let reader = image::ImageReader::with_format(Cursor::new(bytes), image_format);
    reader
        .decode()
        .expect("decode container")
        .to_rgba8()
        .into_raw()
}

fn sample_edits() -> EditableFields {
    EditableFields {
        title: Some("Harbor Dusk".to_owned()),
        description: Some("Wayland test capture".to_owned()),
        artist: Some("GanDesktop QA".to_owned()),
        copyright: Some("CC-BY-4.0".to_owned()),
        rating: Some(4),
        captured_at: Some("2026:11:15 18:30:00".to_owned()),
        gps: Some(GpsCoords {
            latitude: 31.2304,
            longitude: 121.4737,
        }),
    }
}

fn assert_editable_roundtrip(actual: &EditableFields, expected: &EditableFields) {
    assert_eq!(actual.title, expected.title);
    assert_eq!(actual.description, expected.description);
    assert_eq!(actual.artist, expected.artist);
    assert_eq!(actual.copyright, expected.copyright);
    assert_eq!(actual.rating, expected.rating);
    assert_eq!(actual.captured_at, expected.captured_at);
    let actual_gps = actual.gps.expect("gps present after roundtrip");
    let expected_gps = expected.gps.expect("expected gps");
    assert!((actual_gps.latitude - expected_gps.latitude).abs() < 1e-4);
    assert!((actual_gps.longitude - expected_gps.longitude).abs() < 1e-4);
}

fn assert_lossless_container_roundtrip(format: Format) {
    let original = encode_sample(format);
    let original_pixels = decode_rgba(&original, format);
    let edits = sample_edits();

    let updated = rewrite_container_metadata(
        &original,
        format,
        Some(&edits),
        SanitizePolicy::PreserveWithEdits,
    )
    .expect("rewrite with edits");
    let updated_pixels = decode_rgba(&updated, format);
    assert_eq!(
        updated_pixels, original_pixels,
        "{format:?} lossless rewrite must preserve pixels"
    );

    let bundle = inspect_bytes(&updated, format, Some(updated.len() as u64)).expect("inspect");
    assert_editable_roundtrip(&bundle.editable, &edits);
    assert!(
        bundle.technical.has_icc_profile,
        "{format:?} must retain ICC profile on edit"
    );

    let stripped = rewrite_container_metadata(&updated, format, None, SanitizePolicy::StripPrivacy)
        .expect("strip privacy losslessly");
    let stripped_pixels = decode_rgba(&stripped, format);
    assert_eq!(
        stripped_pixels, original_pixels,
        "{format:?} StripPrivacy rewrite must preserve pixels"
    );

    let stripped_bundle =
        inspect_bytes(&stripped, format, Some(stripped.len() as u64)).expect("inspect stripped");
    assert_eq!(stripped_bundle.editable.gps, None);
    assert_eq!(stripped_bundle.editable.artist, None);
    assert_eq!(stripped_bundle.editable.copyright, None);
    assert_eq!(
        stripped_bundle.editable.title.as_deref(),
        Some("Harbor Dusk")
    );
    assert!(
        stripped_bundle.technical.has_icc_profile,
        "{format:?} must retain ICC profile after StripPrivacy"
    );
}

#[test]
fn jpeg_lossless_metadata_rewrite_and_privacy_strip() {
    assert_lossless_container_roundtrip(Format::Jpeg);
}

#[test]
fn png_lossless_metadata_rewrite_and_privacy_strip() {
    assert_lossless_container_roundtrip(Format::Png);
}

#[test]
fn webp_lossless_metadata_rewrite_and_privacy_strip() {
    assert_lossless_container_roundtrip(Format::WebP);
}

fn push_ifd_entry(
    out: &mut Vec<u8>,
    tag: u16,
    field_type: u16,
    count: u32,
    value_or_offset: [u8; 4],
) {
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&field_type.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&value_or_offset);
}

fn synthesize_tiff_with_orientation_thumb_and_serial() -> Vec<u8> {
    // Build a valid Little-Endian TIFF containing:
    // - IFD0: Orientation=6, Make="Fujifilm\0", ExifIFDPointer, GPSIFDPointer, next_ifd -> IFD1
    // - ExifIFD: BodySerialNumber="SN-998877\0"
    // - GPSIFD: GPSLatitudeRef="N\0"
    // - IFD1: JPEGInterchangeFormat + Length pointing to a 4-byte dummy thumbnail
    let mut buf = Vec::new();
    buf.extend_from_slice(b"II*\0");
    buf.extend_from_slice(&8u32.to_le_bytes());

    // IFD0 at offset 8: 4 entries (4*12 = 48) + 2 count + 4 next_ifd = 54 bytes -> ends at 62
    let ifd0_count = 4u16;
    buf.extend_from_slice(&ifd0_count.to_le_bytes());
    let make_str = b"Fujifilm\0\0"; // 10 bytes even
    let make_offset = 62u32;
    let exif_ifd_offset = make_offset + make_str.len() as u32;
    let serial_str = b"SN-998877\0"; // 10 bytes
    let serial_offset = exif_ifd_offset + 2 + 12 + 4;
    let gps_ifd_offset = serial_offset + serial_str.len() as u32;
    let ifd1_offset = gps_ifd_offset + 2 + 12 + 4;
    let thumb_offset = ifd1_offset + 2 + 2 * 12 + 4;

    push_ifd_entry(&mut buf, 0x010F, 2, 9, make_offset.to_le_bytes());
    push_ifd_entry(&mut buf, 0x0112, 3, 1, [6, 0, 0, 0]);
    push_ifd_entry(&mut buf, 0x8769, 4, 1, exif_ifd_offset.to_le_bytes());
    push_ifd_entry(&mut buf, 0x8825, 4, 1, gps_ifd_offset.to_le_bytes());
    buf.extend_from_slice(&ifd1_offset.to_le_bytes());

    // IFD0 extra data
    buf.extend_from_slice(make_str);

    // ExifIFD: 1 entry (0xA431 BodySerialNumber)
    assert_eq!(buf.len() as u32, exif_ifd_offset);
    buf.extend_from_slice(&1u16.to_le_bytes());
    push_ifd_entry(&mut buf, 0xA431, 2, 10, serial_offset.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(serial_str);

    // GPS IFD: 1 entry (0x0001 GPSLatitudeRef = "N\0") + 0x0002? keep simple + 0x0001
    assert_eq!(buf.len() as u32, gps_ifd_offset);
    buf.extend_from_slice(&1u16.to_le_bytes());
    push_ifd_entry(&mut buf, 0x0001, 2, 2, [b'N', 0, 0, 0]);
    buf.extend_from_slice(&0u32.to_le_bytes());

    // IFD1: 2 entries (JPEGInterchangeFormat + Length)
    assert_eq!(buf.len() as u32, ifd1_offset);
    buf.extend_from_slice(&2u16.to_le_bytes());
    push_ifd_entry(&mut buf, 0x0201, 4, 1, thumb_offset.to_le_bytes());
    push_ifd_entry(&mut buf, 0x0202, 4, 1, 4u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xD9]);
    buf
}

#[test]
fn export_exif_resets_orientation_and_drops_ifd1_thumbnail() {
    let raw_tiff = synthesize_tiff_with_orientation_thumb_and_serial();
    let parsed_raw = exif_ifd::parse_exif(&raw_tiff);
    assert_eq!(parsed_raw.orientation, 6);
    assert!(parsed_raw.has_embedded_thumbnail);
    assert_eq!(parsed_raw.exif.camera_make.as_deref(), Some("Fujifilm"));

    let edits = sample_edits();
    let keep_safe = build_export_exif(
        Some(&raw_tiff),
        Some(&edits),
        SanitizePolicy::KeepSafeOnPixelExport,
        Some((120, 80)),
    );
    let parsed_safe = exif_ifd::parse_exif(&keep_safe);
    assert_eq!(parsed_safe.orientation, 1);
    assert!(!parsed_safe.has_embedded_thumbnail);
    assert_eq!(parsed_safe.exif.camera_make.as_deref(), Some("Fujifilm"));
    assert_editable_roundtrip(&parsed_safe.editable, &edits);

    let stripped = build_export_exif(
        Some(&raw_tiff),
        Some(&edits),
        SanitizePolicy::StripPrivacy,
        Some((120, 80)),
    );
    let parsed_stripped = exif_ifd::parse_exif(&stripped);
    assert_eq!(parsed_stripped.orientation, 1);
    assert!(!parsed_stripped.has_embedded_thumbnail);
    assert_eq!(parsed_stripped.editable.gps, None);
    assert_eq!(parsed_stripped.editable.artist, None);
    assert_eq!(parsed_stripped.editable.copyright, None);
    assert_eq!(
        parsed_stripped.exif.camera_make.as_deref(),
        Some("Fujifilm")
    );
    assert!(
        !stripped.windows(9).any(|w| w == b"SN-998877"),
        "serial number must be stripped under StripPrivacy"
    );
}

fn push_be_ifd_entry(
    out: &mut Vec<u8>,
    tag: u16,
    field_type: u16,
    count: u32,
    value_or_offset: [u8; 4],
) {
    out.extend_from_slice(&tag.to_be_bytes());
    out.extend_from_slice(&field_type.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&value_or_offset);
}

fn make_png_chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let crc = crate::service::image::svgz::crc32(&out[4..]);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

fn insert_png_chunk_before_iend(png: &[u8], chunk: &[u8]) -> Vec<u8> {
    let iend_pos = png.len() - 12;
    let mut out = Vec::with_capacity(png.len() + chunk.len());
    out.extend_from_slice(&png[..iend_pos]);
    out.extend_from_slice(chunk);
    out.extend_from_slice(&png[iend_pos..]);
    out
}

fn inject_jpeg_segment_before_eoi(jpeg: &[u8], marker: u8, payload: &[u8]) -> Vec<u8> {
    let eoi_pos = jpeg.len() - 2;
    assert_eq!(&jpeg[eoi_pos..], &[0xFF, 0xD9]);
    let seg_len = (payload.len() + 2) as u16;
    let mut out = Vec::with_capacity(jpeg.len() + payload.len() + 4);
    out.extend_from_slice(&jpeg[..eoi_pos]);
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&seg_len.to_be_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(&[0xFF, 0xD9]);
    out
}

fn compress_zlib_stored(raw: &[u8]) -> Vec<u8> {
    // Valid RFC 1950 zlib stream wrapping a single stored DEFLATE block (`BFINAL=1, BTYPE=0`).
    let mut adler_a = 1u32;
    let mut adler_b = 0u32;
    for &byte in raw {
        adler_a = (adler_a + u32::from(byte)) % 65521;
        adler_b = (adler_b + adler_a) % 65521;
    }
    let adler = (adler_b << 16) | adler_a;
    let len = raw.len() as u16;
    let mut out = Vec::with_capacity(raw.len() + 11);
    out.extend_from_slice(&[0x78, 0x01, 0x01]);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(raw);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

#[test]
fn exif_ifd_blocks_cycles_oom_cap_and_strips_raw_offsets() {
    // Cycle: IFD0 points its ExifIFD pointer back to offset 8 (IFD0 itself).
    let mut cycle = Vec::new();
    cycle.extend_from_slice(b"II*\0");
    cycle.extend_from_slice(&8u32.to_le_bytes());
    cycle.extend_from_slice(&2u16.to_le_bytes());
    push_ifd_entry(&mut cycle, 0x0112, 3, 1, [3, 0, 0, 0]);
    push_ifd_entry(&mut cycle, 0x8769, 4, 1, 8u32.to_le_bytes());
    cycle.extend_from_slice(&8u32.to_le_bytes()); // next_ifd cycle to IFD0
    let parsed_cycle = exif_ifd::parse_exif(&cycle);
    assert_eq!(parsed_cycle.orientation, 3);
    assert!(!parsed_cycle.has_embedded_thumbnail);

    // Aliasing OOM amplification: 10 tags each aliasing the same 1 MiB blob should stop at 4 MiB.
    let mut aliased = vec![0u8; 8 + 2 + 10 * 12 + 4 + 1024 * 1024];
    aliased[..4].copy_from_slice(b"II*\0");
    aliased[4..8].copy_from_slice(&8u32.to_le_bytes());
    aliased[8..10].copy_from_slice(&10u16.to_le_bytes());
    let data_offset = (8 + 2 + 10 * 12 + 4) as u32;
    for idx in 0..10u16 {
        let base = 10 + usize::from(idx) * 12;
        let tag = 0x7000 + idx;
        aliased[base..base + 2].copy_from_slice(&tag.to_le_bytes());
        aliased[base + 2..base + 4].copy_from_slice(&1u16.to_le_bytes()); // BYTE
        aliased[base + 4..base + 8].copy_from_slice(&(1024 * 1024u32).to_le_bytes());
        aliased[base + 8..base + 12].copy_from_slice(&data_offset.to_le_bytes());
    }
    // Re-serializing must fit far below 10 MiB (~4 MiB cap applied during parse).
    let rebuilt = build_export_exif(
        Some(&aliased),
        None,
        SanitizePolicy::PreserveWithEdits,
        None,
    );
    assert!(rebuilt.len() <= 4 * 1024 * 1024 + 4096);

    // Raw TIFF offset-pointer tags (0xA005, 0x0111, 0x0117, 0x0144, 0x0145, 0x014A) must strip.
    let mut ptr_tiff = Vec::new();
    ptr_tiff.extend_from_slice(b"II*\0");
    ptr_tiff.extend_from_slice(&8u32.to_le_bytes());
    ptr_tiff.extend_from_slice(&6u16.to_le_bytes());
    push_ifd_entry(&mut ptr_tiff, 0x0111, 4, 1, 999u32.to_le_bytes());
    push_ifd_entry(&mut ptr_tiff, 0x0112, 3, 1, [1, 0, 0, 0]);
    push_ifd_entry(&mut ptr_tiff, 0x0117, 4, 1, 123u32.to_le_bytes());
    push_ifd_entry(&mut ptr_tiff, 0x0144, 4, 1, 456u32.to_le_bytes());
    push_ifd_entry(&mut ptr_tiff, 0x0145, 4, 1, 789u32.to_le_bytes());
    push_ifd_entry(&mut ptr_tiff, 0x014A, 4, 1, 1000u32.to_le_bytes());
    ptr_tiff.extend_from_slice(&0u32.to_le_bytes());

    let cleaned = build_export_exif(
        Some(&ptr_tiff),
        None,
        SanitizePolicy::PreserveWithEdits,
        None,
    );
    assert!(!cleaned.is_empty());
    let cleaned_parsed = exif_ifd::parse_exif(&cleaned);
    assert_eq!(cleaned_parsed.orientation, 1);
    assert!(!cleaned_parsed.has_embedded_thumbnail);
    // Ensure StripOffsets tag bytes `[0x11, 0x01, 0x04, 0x00]` were removed cleanly.
    assert!(!cleaned.windows(4).any(|w| w == [0x11, 0x01, 0x04, 0x00]));
}

#[test]
fn exif_big_endian_ifd_and_over_512_entries_next_ifd() {
    // Big-Endian "MM\0*" with >512 dummy entries in IFD0 followed by valid next_ifd -> IFD1.
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"MM\0*");
    tiff.extend_from_slice(&8u32.to_be_bytes());

    let entry_count = 515u16;
    tiff.extend_from_slice(&entry_count.to_be_bytes());
    let make_offset = (8 + 2 + usize::from(entry_count) * 12 + 4) as u32;
    let make_bytes = b"Leica\0";
    let ifd1_offset = make_offset + make_bytes.len() as u32;
    let thumb_offset = ifd1_offset + 2 + 2 * 12 + 4;

    push_be_ifd_entry(&mut tiff, 0x010F, 2, 6, make_offset.to_be_bytes());
    push_be_ifd_entry(&mut tiff, 0x0112, 3, 1, [0, 8, 0, 0]);
    for extra in 0..(entry_count - 2) {
        push_be_ifd_entry(&mut tiff, 0xD000 + extra, 3, 1, [0, 1, 0, 0]);
    }
    tiff.extend_from_slice(&ifd1_offset.to_be_bytes());
    tiff.extend_from_slice(make_bytes);

    assert_eq!(tiff.len() as u32, ifd1_offset);
    tiff.extend_from_slice(&2u16.to_be_bytes());
    push_be_ifd_entry(&mut tiff, 0x0201, 4, 1, thumb_offset.to_be_bytes());
    push_be_ifd_entry(&mut tiff, 0x0202, 4, 1, 4u32.to_be_bytes());
    tiff.extend_from_slice(&0u32.to_be_bytes());
    tiff.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xD9]);

    let parsed = exif_ifd::parse_exif(&tiff);
    assert_eq!(parsed.orientation, 8);
    assert_eq!(parsed.exif.camera_make.as_deref(), Some("Leica"));
    assert!(
        parsed.has_embedded_thumbnail,
        "next_ifd must be read after raw_count*12 even when raw_count > 512"
    );

    // Big-Endian TYPE_IFD (13) ExifIFD pointer must parse child DateTimeOriginal, while ASCII
    // in 0x8769 must not be misread as an integer IFD offset.
    let mut ifd13_tiff = Vec::new();
    ifd13_tiff.extend_from_slice(b"MM\0*");
    ifd13_tiff.extend_from_slice(&8u32.to_be_bytes());
    ifd13_tiff.extend_from_slice(&1u16.to_be_bytes());
    let sub_ifd_off = 8u32 + 2 + 12 + 4;
    push_be_ifd_entry(&mut ifd13_tiff, 0x8769, 13, 1, sub_ifd_off.to_be_bytes());
    ifd13_tiff.extend_from_slice(&0u32.to_be_bytes());
    ifd13_tiff.extend_from_slice(&1u16.to_be_bytes());
    let date_off = sub_ifd_off + 2 + 12 + 4;
    push_be_ifd_entry(&mut ifd13_tiff, 0x9003, 2, 20, date_off.to_be_bytes());
    ifd13_tiff.extend_from_slice(&0u32.to_be_bytes());
    ifd13_tiff.extend_from_slice(b"2026:11:16 03:50:00\0");
    let ifd13_parsed = exif_ifd::parse_exif(&ifd13_tiff);
    assert_eq!(
        ifd13_parsed.editable.captured_at.as_deref(),
        Some("2026:11:16 03:50:00")
    );

    let mut ascii_ptr_tiff = ifd13_tiff.clone();
    // Overwrite field_type at bytes 10..12 with TYPE_ASCII (2).
    ascii_ptr_tiff[10..12].copy_from_slice(&2u16.to_be_bytes());
    assert_eq!(
        exif_ifd::parse_exif(&ascii_ptr_tiff).editable.captured_at,
        None
    );
}

#[test]
fn gps_rollover_invalid_gps_and_privacy_tags() {
    // Coordinate near integer boundary rounds seconds cleanly to next degree/minute without 60.0".
    let edits = EditableFields {
        artist: Some("Canonical Author".to_owned()),
        gps: Some(GpsCoords {
            latitude: 35.999_999_99,
            longitude: -120.999_999_99,
        }),
        ..Default::default()
    };
    let built = build_export_exif(None, Some(&edits), SanitizePolicy::PreserveWithEdits, None);
    let parsed = exif_ifd::parse_exif(&built);
    let gps = parsed.editable.gps.expect("valid rolled-over gps");
    assert!((gps.latitude - 36.0).abs() < 1e-6);
    assert!((gps.longitude + 121.0).abs() < 1e-6);
    // Ensure serialized DMS does not contain `600_000` (60.0000s) in sec_scaled.
    assert!(
        !built
            .windows(8)
            .any(|w| w[..4] == 600_000u32.to_le_bytes() && w[4..8] == 10_000u32.to_le_bytes())
    );

    // Invalid GPS LatitudeRef "X" and XPComment/XPAuthor/CameraSerialNumber privacy strip.
    let mut bad_gps = Vec::new();
    bad_gps.extend_from_slice(b"II*\0");
    bad_gps.extend_from_slice(&8u32.to_le_bytes());
    bad_gps.extend_from_slice(&3u16.to_le_bytes());
    // XPAuthor UTF-16LE "Old\0" -> inline 4 bytes "O\0l\0" or offset; put XPComment 0x9C9C, GPS ptr
    push_ifd_entry(&mut bad_gps, 0x9C9C, 1, 4, [b'S', 0, b'e', 0]);
    push_ifd_entry(&mut bad_gps, 0x9C9D, 1, 4, [b'X', 0, b'Y', 0]);
    let gps_off = (8 + 2 + 3 * 12 + 4) as u32;
    push_ifd_entry(&mut bad_gps, 0x8825, 4, 1, gps_off.to_le_bytes());
    bad_gps.extend_from_slice(&0u32.to_le_bytes());
    // GPSIFD with invalid LatRef "X\0"
    let deg_off = gps_off + 2 + 4 * 12 + 4;
    bad_gps.extend_from_slice(&4u16.to_le_bytes());
    push_ifd_entry(&mut bad_gps, 0x0001, 2, 2, [b'X', 0, 0, 0]);
    push_ifd_entry(&mut bad_gps, 0x0002, 5, 3, deg_off.to_le_bytes());
    push_ifd_entry(&mut bad_gps, 0x0003, 2, 2, [b'E', 0, 0, 0]);
    push_ifd_entry(&mut bad_gps, 0x0004, 5, 3, deg_off.to_le_bytes());
    bad_gps.extend_from_slice(&0u32.to_le_bytes());
    for (n, d) in [(10u32, 1u32), (0, 1), (0, 1)] {
        bad_gps.extend_from_slice(&n.to_le_bytes());
        bad_gps.extend_from_slice(&d.to_le_bytes());
    }

    let inspected = exif_ifd::parse_exif(&bad_gps);
    assert_eq!(
        inspected.editable.gps, None,
        "invalid GPSLatitudeRef must yield None"
    );

    // Edits clearing artist must also drop stale XPAuthor.
    let cleared = build_export_exif(
        Some(&bad_gps),
        Some(&EditableFields::default()),
        SanitizePolicy::PreserveWithEdits,
        None,
    );
    assert!(!cleared.windows(2).any(|w| w == [0x9D, 0x9C]));
    // StripPrivacy drops XPComment & XPAuthor.
    let stripped = build_export_exif(Some(&bad_gps), None, SanitizePolicy::StripPrivacy, None);
    assert!(!stripped.windows(2).any(|w| w == [0x9C, 0x9C]));
    assert!(!stripped.windows(2).any(|w| w == [0x9D, 0x9C]));
}

#[test]
fn jpeg_rejects_malformed_and_strips_post_sos_privacy_markers() {
    let valid = encode_sample(Format::Jpeg);

    // Missing SOS marker: SOI + APP0 + EOI.
    let missing_sos = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xD9];
    assert!(
        rewrite_container_metadata(
            &missing_sos,
            Format::Jpeg,
            None,
            SanitizePolicy::StripPrivacy
        )
        .is_err()
    );

    // Invalid marker prefix (non-0xFF byte right after SOI).
    let bad_prefix = [0xFF, 0xD8, 0x12, 0x34, 0xFF, 0xDA, 0x00, 0x02, 0xFF, 0xD9];
    assert!(
        rewrite_container_metadata(
            &bad_prefix,
            Format::Jpeg,
            None,
            SanitizePolicy::PreserveWithEdits
        )
        .is_err()
    );

    // Inject post-SOS COM (0xFE) and post-SOS XMP APP1 right before EOI.
    let with_com = inject_jpeg_segment_before_eoi(&valid, 0xFE, b"SECRET_POST_SOS_COMMENT");
    let mut xmp_payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    xmp_payload.extend_from_slice(b"<xmp>PRIVATE_POST_SOS</xmp>");
    let with_both = inject_jpeg_segment_before_eoi(&with_com, 0xE1, &xmp_payload);

    let stripped =
        rewrite_container_metadata(&with_both, Format::Jpeg, None, SanitizePolicy::StripPrivacy)
            .expect("strip post-SOS segments");
    assert!(
        !stripped
            .windows(23)
            .any(|w| w == b"SECRET_POST_SOS_COMMENT")
    );
    assert!(!stripped.windows(16).any(|w| w == b"PRIVATE_POST_SOS"));
    assert_eq!(
        decode_rgba(&stripped, Format::Jpeg),
        decode_rgba(&valid, Format::Jpeg)
    );

    // JPEG with 0xFF fill byte before RST0 (0xFF 0xFF 0xD0) inside SOS scan + post-SOS COM.
    let sos_pos = valid
        .windows(2)
        .position(|w| w == [0xFF, 0xDA])
        .expect("SOS");
    let sos_len = usize::from(u16::from_be_bytes([valid[sos_pos + 2], valid[sos_pos + 3]]));
    let ecs_start = sos_pos + 2 + sos_len;
    let eoi_pos = valid.len() - 2;
    let mut with_rst_fill = Vec::new();
    with_rst_fill.extend_from_slice(&valid[..ecs_start]);
    with_rst_fill.extend_from_slice(&[0xFF, 0xFF, 0xD0, 0x12]); // fill + RST0 + scan byte
    with_rst_fill.extend_from_slice(&valid[ecs_start..eoi_pos]);
    with_rst_fill.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x06, b'H', b'I', b'D', b'E']);
    with_rst_fill.extend_from_slice(&[0xFF, 0xD9]);
    let rst_stripped = rewrite_container_metadata(
        &with_rst_fill,
        Format::Jpeg,
        None,
        SanitizePolicy::StripPrivacy,
    )
    .expect("rewrite JPEG with 0xFF fill before RST0");
    assert!(!rst_stripped.windows(4).any(|w| w == b"HIDE"));
    assert!(
        rst_stripped
            .windows(4)
            .any(|w| w == [0xFF, 0xFF, 0xD0, 0x12]),
        "RST0 with fill byte inside ECS must be preserved"
    );

    // Malformed fill 0xFF followed by stuffed 0x00 in ECS must be rejected.
    let mut bad_fill = Vec::new();
    bad_fill.extend_from_slice(&valid[..ecs_start]);
    bad_fill.extend_from_slice(&[0xFF, 0xFF, 0x00]);
    bad_fill.extend_from_slice(&valid[ecs_start..]);
    assert!(
        rewrite_container_metadata(&bad_fill, Format::Jpeg, None, SanitizePolicy::StripPrivacy)
            .is_err()
    );
}

#[test]
fn png_validates_crc_decodes_latin1_ztxt_compressed_itxt_and_strips_xmp() {
    let valid = encode_sample(Format::Png);

    // Corrupt last CRC byte of IDAT/chunk.
    let mut bad_crc = valid.clone();
    let crc_idx = bad_crc.len() - 13;
    bad_crc[crc_idx] ^= 0x55;
    assert!(
        rewrite_container_metadata(
            &bad_crc,
            Format::Png,
            None,
            SanitizePolicy::PreserveWithEdits
        )
        .is_err()
    );

    // PNG missing IDAT.
    let mut no_idat = Vec::new();
    no_idat.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    no_idat.extend_from_slice(&valid[8..33]); // IHDR
    no_idat.extend_from_slice(&make_png_chunk(*b"IEND", &[]));
    assert!(
        rewrite_container_metadata(
            &no_idat,
            Format::Png,
            None,
            SanitizePolicy::PreserveWithEdits
        )
        .is_err()
    );

    // ISO-8859-1 tEXt chunk: "Copyright\0© 2026" (0xA9 in Latin-1)
    let text_chunk = make_png_chunk(*b"tEXt", b"Copyright\0\xA9 2026");
    // zTXt chunk: "Author\0\0" + compressed "Alice"
    let mut ztxt_data = b"Author\0\0".to_vec();
    ztxt_data.extend_from_slice(&compress_zlib_stored(b"Alice"));
    let ztxt_chunk = make_png_chunk(*b"zTXt", &ztxt_data);
    // Compressed post-IDAT iTXt chunk: "Title\0\x01\0\0\0" + compressed UTF-8 "Café"
    let mut itxt_data = b"Title\0\x01\0\0\0".to_vec();
    itxt_data.extend_from_slice(&compress_zlib_stored("Café".as_bytes()));
    let itxt_title_chunk = make_png_chunk(*b"iTXt", &itxt_data);
    // XMP iTXt chunk
    let xmp_chunk = make_png_chunk(
        *b"iTXt",
        b"XML:com.adobe.xmp\0\0\0\0\0<x:xmpmeta>STALE_XMP</x:xmpmeta>",
    );

    let with_text = insert_png_chunk_before_iend(&valid, &text_chunk);
    let with_ztxt = insert_png_chunk_before_iend(&with_text, &ztxt_chunk);
    let with_itxt = insert_png_chunk_before_iend(&with_ztxt, &itxt_title_chunk);
    let full = insert_png_chunk_before_iend(&with_itxt, &xmp_chunk);

    let bundle = inspect_bytes(&full, Format::Png, Some(full.len() as u64)).expect("inspect png");
    assert_eq!(bundle.editable.copyright.as_deref(), Some("© 2026"));
    assert_eq!(bundle.editable.artist.as_deref(), Some("Alice"));
    assert_eq!(bundle.editable.title.as_deref(), Some("Café"));

    // Updating editable fields must strip stale XML:com.adobe.xmp iTXt chunk.
    let edited = rewrite_container_metadata(
        &full,
        Format::Png,
        Some(&EditableFields {
            title: Some("New Title".to_owned()),
            ..Default::default()
        }),
        SanitizePolicy::PreserveWithEdits,
    )
    .expect("rewrite png with edits");
    assert!(!edited.windows(9).any(|w| w == b"STALE_XMP"));
}

#[test]
fn webp_rejects_truncated_tail_and_orders_xmp_after_exif() {
    let valid = encode_sample(Format::WebP);

    // Truncated 3 trailing tail bytes.
    let mut with_tail = valid.clone();
    with_tail.extend_from_slice(&[0x01, 0x02, 0x03]);
    assert!(
        rewrite_container_metadata(
            &with_tail,
            Format::WebP,
            None,
            SanitizePolicy::PreserveWithEdits
        )
        .is_err()
    );

    // Missing odd-chunk pad byte.
    let mut missing_pad = valid.clone();
    missing_pad.extend_from_slice(b"JUNK");
    missing_pad.extend_from_slice(&3u32.to_le_bytes());
    missing_pad.extend_from_slice(b"abc"); // size=3 requires 1 trailing pad byte!
    assert!(
        rewrite_container_metadata(
            &missing_pad,
            Format::WebP,
            None,
            SanitizePolicy::PreserveWithEdits
        )
        .is_err()
    );

    // WebP with edits preserves valid chunk order: EXIF before XMP when XMP exists before image.
    let edits = sample_edits();
    let with_exif = rewrite_container_metadata(
        &valid,
        Format::WebP,
        Some(&edits),
        SanitizePolicy::PreserveWithEdits,
    )
    .expect("add EXIF");
    // Append an XMP chunk right before EXIF manually and rewrite with edits=None, PreserveWithEdits:
    let exif_pos = with_exif
        .windows(4)
        .position(|w| w == b"EXIF")
        .expect("EXIF chunk");
    let mut misordered = Vec::new();
    misordered.extend_from_slice(&with_exif[..exif_pos]);
    misordered.extend_from_slice(b"JUNK");
    misordered.extend_from_slice(&4u32.to_le_bytes());
    misordered.extend_from_slice(b"abcd");
    misordered.extend_from_slice(b"XMP ");
    misordered.extend_from_slice(&4u32.to_le_bytes());
    misordered.extend_from_slice(b"xmp!");
    misordered.extend_from_slice(&with_exif[exif_pos..]);
    let riff_body_len = (misordered.len() - 8) as u32;
    misordered[4..8].copy_from_slice(&riff_body_len.to_le_bytes());

    let canonical = rewrite_container_metadata(
        &misordered,
        Format::WebP,
        None,
        SanitizePolicy::PreserveWithEdits,
    )
    .expect("canonicalize WebP order");
    let new_exif_pos = canonical
        .windows(4)
        .position(|w| w == b"EXIF")
        .expect("EXIF");
    let new_xmp_pos = canonical
        .windows(4)
        .position(|w| w == b"XMP ")
        .expect("XMP");
    let new_junk_pos = canonical
        .windows(4)
        .position(|w| w == b"JUNK")
        .expect("JUNK");
    assert!(
        new_exif_pos < new_xmp_pos && new_xmp_pos < new_junk_pos,
        "RFC 9649 requires EXIF before XMP and unknown chunks after XMP"
    );
}

#[test]
fn inspect_bytes_enforces_limits_and_reads_gif_frame_count() {
    // Minimal 2-frame valid GIF89a (1x1)
    let gif_2_frames: Vec<u8> = vec![
        b'G', b'I', b'F', b'8', b'9', b'a', // header
        1, 0, 1, 0, 0x00, 0, 0, // LSD 1x1, no GCT
        0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0, // frame 1 descriptor
        2, 2, 0x4C, 0x01, 0, // LZW min=2, 2 bytes, terminator
        0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0, // frame 2 descriptor
        2, 2, 0x4C, 0x01, 0,    // LZW data
        0x3B, // trailer
    ];
    let gif_meta =
        inspect_bytes(&gif_2_frames, Format::Gif, Some(gif_2_frames.len() as u64)).expect("gif");
    assert_eq!(gif_meta.technical.frame_count, Some(2));

    // Craft PNG IHDR claiming 100_000 x 100_000 (exceeds MAX_INSPECT_EDGE_PX = 65_536).
    let mut huge_ihdr = [0u8; 13];
    huge_ihdr[0..4].copy_from_slice(&100_000u32.to_be_bytes());
    huge_ihdr[4..8].copy_from_slice(&100_000u32.to_be_bytes());
    huge_ihdr[8] = 8;
    huge_ihdr[9] = 6;
    let mut bad_png = vec![137, 80, 78, 71, 13, 10, 26, 10];
    bad_png.extend_from_slice(&make_png_chunk(*b"IHDR", &huge_ihdr));
    bad_png.extend_from_slice(&make_png_chunk(*b"IEND", &[]));
    assert!(inspect_bytes(&bad_png, Format::Png, Some(bad_png.len() as u64)).is_err());
}
