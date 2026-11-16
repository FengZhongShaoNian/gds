use std::io::Cursor;

use image::{ImageEncoder as _, Rgba, RgbaImage};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::service::image::metadata::MAX_SVG_XML_BYTES;

fn gzip_wrap_stored(raw: &[u8]) -> Vec<u8> {
    assert!(raw.len() <= usize::from(u16::MAX));
    let mut out = Vec::with_capacity(raw.len() + 23);
    out.extend_from_slice(&[0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xFF]);
    out.push(0x01); // BFINAL=1, BTYPE=00 (stored)
    let len = raw.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(raw);
    let crc = crate::service::image::svgz::crc32(raw);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out
}

fn push_bits_lsb(out: &mut Vec<u8>, bit_buf: &mut u64, bit_cnt: &mut u8, val: u32, bits: u8) {
    *bit_buf |= u64::from(val) << *bit_cnt;
    *bit_cnt += bits;
    while *bit_cnt >= 8 {
        out.push((*bit_buf & 0xFF) as u8);
        *bit_buf >>= 8;
        *bit_cnt -= 8;
    }
}

fn reverse_bits(mut val: u32, width: u8) -> u32 {
    let mut res = 0u32;
    for _ in 0..width {
        res = (res << 1) | (val & 1);
        val >>= 1;
    }
    res
}

fn gzip_fixed_huffman_repeated_spaces(total_len: usize) -> Vec<u8> {
    let mut out = vec![0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xFF];
    let mut bit_buf = 0u64;
    let mut bit_cnt = 0u8;

    push_bits_lsb(&mut out, &mut bit_buf, &mut bit_cnt, 0b011, 3); // BFINAL=1, BTYPE=01 (fixed)
    // Literal `' '` (0x20 -> fixed code 48 + 32 = 80, 8 bits MSB-first)
    push_bits_lsb(&mut out, &mut bit_buf, &mut bit_cnt, reverse_bits(80, 8), 8);
    let repeats = total_len.saturating_sub(1) / 258 + 1;
    let len_258_code = reverse_bits(0xC5, 8); // symbol 285 -> 0b1100_0101
    for _ in 0..repeats {
        push_bits_lsb(&mut out, &mut bit_buf, &mut bit_cnt, len_258_code, 8);
        push_bits_lsb(&mut out, &mut bit_buf, &mut bit_cnt, 0, 5); // distance 1 -> symbol 0
    }
    push_bits_lsb(&mut out, &mut bit_buf, &mut bit_cnt, 0, 7); // EOB symbol 256
    if bit_cnt > 0 {
        out.push((bit_buf & 0xFF) as u8);
    }
    out.extend_from_slice(&[0; 8]); // trailer unused because size cap triggers during inflate
    out
}

#[test]
fn svgz_valid_decodes_and_gzip_bomb_fails_with_too_large() {
    let svg_xml = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect x="0" y="0" width="200" height="100" fill="#2060c0" fill-opacity="0.5"/></svg>"##;
    let svgz = gzip_wrap_stored(svg_xml);

    let origin = Origin::Memory {
        kind: super::super::source::MemoryOrigin::Clipboard,
        bytes: std::sync::Arc::from(svgz),
        format_hint: Some(Format::Svg),
    };
    let cancel = CancellationToken::new();
    let preview = decode_preview(&origin, 50, DecodeLimits::default(), &cancel)
        .expect("decode valid svgz preview");

    assert!(preview.is_preview_downscaled);
    assert_eq!(preview.rgba.dimensions(), (50, 25));
    // Intrinsic SVG metadata dimensions must stay at 200x100 even on downscaled preview.
    assert_eq!(preview.metadata.technical.width, 200);
    assert_eq!(preview.metadata.technical.height, 100);
    // Straight alpha demultiplication must keep RGB close to `#2060c0` `[32, 96, 192]`.
    let px = preview.rgba.get_pixel(25, 12).0;
    assert!((i16::from(px[0]) - 32).abs() <= 2);
    assert!((i16::from(px[1]) - 96).abs() <= 2);
    assert!((i16::from(px[2]) - 192).abs() <= 2);
    assert!(px[3] >= 126 && px[3] <= 129);

    // Gzip bomb exceeding MAX_SVG_XML_BYTES must reject deterministically with Error::TooLarge.
    let bomb = gzip_fixed_huffman_repeated_spaces(MAX_SVG_XML_BYTES + 4096);
    let bomb_origin = Origin::Memory {
        kind: super::super::source::MemoryOrigin::Clipboard,
        bytes: std::sync::Arc::from(bomb),
        format_hint: Some(Format::Svg),
    };
    let err = decode_origin(&bomb_origin, DecodeLimits::default(), &cancel)
        .expect_err("svgz bomb must be rejected");
    assert!(matches!(err, Error::TooLarge { .. }));

    // Dynamic-Huffman (BTYPE=2) gzip-compressed SVG fixture exercising `read_dynamic_tables`.
    let dynamic_svgz: Vec<u8> = vec![
        31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 157, 142, 193, 10, 131, 64, 12, 68, 127, 37, 164, 247,
        77, 106, 91, 40, 197, 248, 47, 98, 237, 238, 194, 90, 69, 131, 177, 127, 223, 93, 207, 158,
        188, 12, 204, 240, 102, 152, 122, 89, 61, 108, 67, 250, 46, 130, 65, 117, 122, 17, 153,
        153, 179, 155, 27, 103, 79, 21, 51, 83, 38, 16, 44, 190, 53, 8, 62, 25, 33, 244, 209, 7,
        21, 188, 51, 54, 245, 220, 119, 10, 155, 96, 206, 127, 187, 30, 131, 240, 137, 41, 9, 94,
        174, 220, 114, 182, 116, 182, 8, 227, 212, 118, 81, 75, 195, 61, 202, 76, 121, 215, 252, 1,
        87, 87, 115, 237, 197, 0, 0, 0,
    ];
    assert_eq!((dynamic_svgz[10] >> 1) & 0b11, 2, "must exercise BTYPE=2");
    let dynamic_origin = Origin::Memory {
        kind: super::super::source::MemoryOrigin::Clipboard,
        bytes: std::sync::Arc::from(dynamic_svgz),
        format_hint: Some(Format::Svg),
    };
    let dynamic_decoded =
        decode_origin(&dynamic_origin, DecodeLimits::default(), &cancel).expect("BTYPE=2 svgz");
    assert_eq!(dynamic_decoded.rgba.dimensions(), (80, 40));

    // Concatenated valid two-member .svgz succeeds.
    let mut multi_valid =
        gzip_wrap_stored(br##"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12">"##);
    multi_valid.extend_from_slice(&gzip_wrap_stored(br##"</svg>"##));
    let multi_origin = Origin::Memory {
        kind: super::super::source::MemoryOrigin::Clipboard,
        bytes: std::sync::Arc::from(multi_valid),
        format_hint: Some(Format::Svg),
    };
    let multi_decoded =
        decode_origin(&multi_origin, DecodeLimits::default(), &cancel).expect("multi-member svgz");
    assert_eq!(multi_decoded.rgba.dimensions(), (12, 12));

    // Member 2 cannot back-reference bytes from Member 1's window.
    let mut cross_window =
        gzip_wrap_stored(br##"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12">"##);
    cross_window.extend_from_slice(&[0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xFF]);
    let mut bit_buf = 0u64;
    let mut bit_cnt = 0u8;
    push_bits_lsb(&mut cross_window, &mut bit_buf, &mut bit_cnt, 0b011, 3);
    push_bits_lsb(
        &mut cross_window,
        &mut bit_buf,
        &mut bit_cnt,
        reverse_bits(1, 7),
        7,
    ); // length sym 257 (len=3) at output pos 0 of member 2
    push_bits_lsb(&mut cross_window, &mut bit_buf, &mut bit_cnt, 0, 5); // distance=1
    push_bits_lsb(&mut cross_window, &mut bit_buf, &mut bit_cnt, 0, 7); // EOB 256
    if bit_cnt > 0 {
        cross_window.push((bit_buf & 0xFF) as u8);
    }
    cross_window.extend_from_slice(&[0; 8]);
    let cross_origin = Origin::Memory {
        kind: super::super::source::MemoryOrigin::Clipboard,
        bytes: std::sync::Arc::from(cross_window),
        format_hint: Some(Format::Svg),
    };
    let cross_err = decode_origin(&cross_origin, DecodeLimits::default(), &cancel)
        .expect_err("cross-member back-reference must fail");
    assert!(matches!(cross_err, Error::Svg(_)));
}

#[test]
fn decode_preview_swaps_metadata_dimensions_for_rotated_exif_images() {
    let mut src = RgbaImage::from_pixel(16, 8, Rgba([10, 20, 30, 255]));
    src.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    let mut encoded = Vec::new();
    {
        let encoder = image::codecs::png::PngEncoder::new(&mut encoded);
        encoder
            .write_image(src.as_raw(), 16, 8, image::ExtendedColorType::Rgba8)
            .expect("encode png");
    }
    // Synthesize minimal LE EXIF TIFF with Orientation = 6 (90 deg CW -> coded 16x8 becomes 8x16).
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II*\0");
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&[6, 0, 0, 0]);
    tiff.extend_from_slice(&0u32.to_le_bytes());

    let mut with_exif = Vec::new();
    // Copy PNG signature + IHDR, inject eXIf, copy IDAT + IEND.
    with_exif.extend_from_slice(&encoded[..33]);
    with_exif.extend_from_slice(&(tiff.len() as u32).to_be_bytes());
    let crc_start = with_exif.len();
    with_exif.extend_from_slice(b"eXIf");
    with_exif.extend_from_slice(&tiff);
    let crc = crate::service::image::svgz::crc32(&with_exif[crc_start..]);
    with_exif.extend_from_slice(&crc.to_be_bytes());
    with_exif.extend_from_slice(&encoded[33..]);

    let origin = Origin::Memory {
        kind: super::super::source::MemoryOrigin::Stdin,
        bytes: std::sync::Arc::from(with_exif),
        format_hint: Some(Format::Png),
    };
    let preview = decode_preview(
        &origin,
        8,
        DecodeLimits::default(),
        &CancellationToken::new(),
    )
    .expect("decode rotated preview");
    assert!(preview.is_preview_downscaled);
    assert_eq!(preview.rgba.dimensions(), (4, 8));
    assert_eq!(
        (
            preview.metadata.technical.width,
            preview.metadata.technical.height
        ),
        (8, 16)
    );
}

#[test]
fn decode_origin_enforces_icc_cms_limits_cancellation_and_local_fd_safety() {
    let mut src = RgbaImage::from_pixel(4, 4, Rgba([200, 100, 50, 255]));
    src.put_pixel(0, 0, Rgba([255, 255, 255, 255]));
    let mut encoded = Vec::new();
    let icc = moxcms::ColorProfile::new_srgb()
        .encode()
        .expect("encode srgb icc");
    {
        let mut encoder = image::codecs::png::PngEncoder::new(&mut encoded);
        encoder.set_icc_profile(icc).expect("set icc");
        encoder
            .write_image(src.as_raw(), 4, 4, image::ExtendedColorType::Rgba8)
            .expect("encode png");
    }

    let origin = Origin::Memory {
        kind: super::super::source::MemoryOrigin::Clipboard,
        bytes: std::sync::Arc::from(encoded.clone()),
        format_hint: None,
    };
    let cancel = CancellationToken::new();
    let decoded = decode_origin(&origin, DecodeLimits::default(), &cancel).expect("decode icc");
    assert!(decoded.was_color_managed);
    assert_eq!(decoded.rgba.dimensions(), (4, 4));

    // Edge limit produces DimensionsTooLarge.
    let small_edge = DecodeLimits {
        max_edge_px: 2,
        ..Default::default()
    };
    let dim_err = decode_origin(&origin, small_edge, &cancel).expect_err("edge limit should fail");
    assert!(matches!(dim_err, Error::DimensionsTooLarge { .. }));

    // Cancelled token returns Error::Cancelled.
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        decode_origin(&origin, DecodeLimits::default(), &cancelled),
        Err(Error::Cancelled)
    ));

    // Local file symlink & FIFO protection + FD FileStamp capture.
    let dir = tempfile::tempdir().expect("tempdir");
    let real_path = dir.path().join("real.png");
    std::fs::write(&real_path, &encoded).expect("write real png");
    let decoded_local = decode_origin(
        &Origin::local(real_path.clone()),
        DecodeLimits::default(),
        &cancel,
    )
    .expect("decode local png");
    assert!(matches!(
        decoded_local.origin,
        Origin::LocalFile { stamp: Some(_), .. }
    ));

    #[cfg(unix)]
    {
        let link = dir.path().join("sym.png");
        std::os::unix::fs::symlink(&real_path, &link).expect("symlink");
        let sym_decoded = decode_origin(&Origin::local(link), DecodeLimits::default(), &cancel)
            .expect("symlinked regular image must open cleanly");
        assert!(matches!(
            sym_decoded.origin,
            Origin::LocalFile { stamp: None, .. }
        ));
        assert_eq!(sym_decoded.rgba.dimensions(), (4, 4));

        let fifo = dir.path().join("fifo.png");
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::from_bits_truncate(0o600))
            .expect("mkfifo");
        let fifo_err = decode_origin(
            &Origin::local(fifo.clone()),
            DecodeLimits::default(),
            &cancel,
        )
        .unwrap_err();
        assert!(matches!(fifo_err, Error::Io { .. }));

        let fifo_sym = dir.path().join("sym_fifo.png");
        std::os::unix::fs::symlink(&fifo, &fifo_sym).expect("symlink to fifo");
        let fifo_sym_err =
            decode_origin(&Origin::local(fifo_sym), DecodeLimits::default(), &cancel).unwrap_err();
        assert!(matches!(fifo_sym_err, Error::Io { .. }));
    }

    // Sanity check decoded buffer matches original dimensions via cursor decode too.
    let dec = image::ImageReader::new(Cursor::new(&decoded.source_bytes))
        .with_guessed_format()
        .expect("format")
        .decode()
        .expect("decode");
    assert_eq!(dec.width(), 4);
}
