//! Lossless container chunk rewriters for JPEG, PNG, and WebP metadata updates.

use png::text_metadata::EncodableTextChunk as _;

use crate::service::image::{Error, source::Format, svgz};

use super::{
    EditableFields, SanitizePolicy,
    exif_ifd::{TiffBuildOptions, build_exif_tiff, strip_exif_header},
};

const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
const PNG_IEND_CHUNK: [u8; 12] = [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82];
const JPEG_EXIF_HEADER: &[u8; 6] = b"Exif\0\0";
const MAX_PNG_TEXT_BYTES: usize = 1024 * 1024;
const MAX_PNG_CHUNK_LEN: usize = 0x7FFF_FFFF;

/// Rewrites metadata chunks in a JPEG, PNG, or WebP container without re-encoding pixels.
///
/// # Errors
/// Returns [`Error::UnsupportedOperation`] for formats without lossless container rewriting or
/// [`Error::InvalidMetadata`] when the container framing is truncated or malformed.
pub fn rewrite_container_metadata(
    container_bytes: &[u8],
    format: Format,
    edits: Option<&EditableFields>,
    policy: SanitizePolicy,
) -> Result<Vec<u8>, Error> {
    if !format.capabilities().can_losslessly_rewrite_metadata {
        return Err(Error::UnsupportedOperation(
            "format does not support lossless metadata container rewriting",
        ));
    }
    match format {
        Format::Jpeg => rewrite_jpeg(container_bytes, edits, policy),
        Format::Png => rewrite_png(container_bytes, edits, policy),
        Format::WebP => rewrite_webp(container_bytes, edits, policy),
        _ => Err(Error::UnsupportedOperation(
            "format does not support lossless metadata container rewriting",
        )),
    }
}

pub(super) fn inspect_png_text_chunks(bytes: &[u8], fields: &mut EditableFields) {
    if bytes.len() < 8 || bytes[..8] != PNG_SIG {
        return;
    }

    let mut pos = 8usize;
    let mut ihdr_raw: Option<&[u8]> = None;
    let mut text_chunks: Vec<&[u8]> = Vec::new();
    let mut raw_budget = MAX_PNG_TEXT_BYTES;

    while let Some(chunk) = read_png_chunk_unchecked(bytes, &mut pos) {
        if chunk.kind == *b"IHDR" && ihdr_raw.is_none() {
            ihdr_raw = Some(chunk.raw);
        } else if matches!(&chunk.kind, b"tEXt" | b"iTXt" | b"zTXt")
            && chunk.data.len() <= raw_budget
        {
            raw_budget -= chunk.data.len();
            text_chunks.push(chunk.raw);
        }
        if chunk.kind == *b"IEND" {
            break;
        }
    }

    let Some(ihdr) = ihdr_raw else {
        return;
    };
    let mut decoder = png::StreamingDecoder::new();
    if !feed_png_bytes(&mut decoder, &PNG_SIG) || !feed_png_bytes(&mut decoder, ihdr) {
        return;
    }
    for raw in text_chunks {
        if !feed_png_bytes(&mut decoder, raw) {
            return;
        }
    }
    let _ = feed_png_bytes(&mut decoder, &PNG_IEND_CHUNK);

    let Some(info) = decoder.info() else {
        return;
    };
    let mut text_budget = MAX_PNG_TEXT_BYTES;

    for chunk in &info.utf8_text {
        let mut owned = chunk.clone();
        if owned.decompress_text_with_limit(text_budget).is_ok()
            && let Ok(text) = owned.get_text()
        {
            text_budget = text_budget.saturating_sub(text.len());
            assign_png_text_field(fields, &owned.keyword, text);
        }
    }
    for chunk in &info.uncompressed_latin1_text {
        assign_png_text_field(fields, &chunk.keyword, chunk.text.clone());
    }
    for chunk in &info.compressed_latin1_text {
        let mut owned = chunk.clone();
        if owned.decompress_text_with_limit(text_budget).is_ok()
            && let Ok(text) = owned.get_text()
        {
            text_budget = text_budget.saturating_sub(text.len());
            assign_png_text_field(fields, &owned.keyword, text);
        }
    }
}

fn feed_png_bytes(decoder: &mut png::StreamingDecoder, mut slice: &[u8]) -> bool {
    while !slice.is_empty() {
        match decoder.update(slice, None) {
            Ok((consumed, _)) if consumed > 0 => slice = &slice[consumed..],
            Ok(_) | Err(_) => return false,
        }
    }
    true
}

fn assign_png_text_field(fields: &mut EditableFields, key: &str, value: String) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    let owned = trimmed.to_owned();
    match key {
        "Title" if fields.title.is_none() => fields.title = Some(owned),
        "Description" if fields.description.is_none() => fields.description = Some(owned),
        "Author" if fields.artist.is_none() => fields.artist = Some(owned),
        "Copyright" if fields.copyright.is_none() => fields.copyright = Some(owned),
        _ => {}
    }
}

fn rewrite_jpeg(
    bytes: &[u8],
    edits: Option<&EditableFields>,
    policy: SanitizePolicy,
) -> Result<Vec<u8>, Error> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Err(Error::InvalidMetadata("missing JPEG SOI marker"));
    }

    let mut existing_exif: Option<&[u8]> = None;
    let mut pre_sos: Vec<&[u8]> = Vec::new();
    let mut from_sos: Vec<&[u8]> = Vec::new();
    let mut seen_sos = false;
    let mut seen_eoi = false;
    let mut pos = 2usize;

    while pos < bytes.len() {
        if bytes[pos] != 0xFF {
            return Err(Error::InvalidMetadata("missing JPEG marker 0xFF prefix"));
        }
        while pos < bytes.len() && bytes[pos] == 0xFF {
            pos += 1;
        }
        let Some(&marker) = bytes.get(pos) else {
            return Err(Error::InvalidMetadata("truncated JPEG marker"));
        };
        let marker_pair_start = pos - 1;
        pos += 1;

        if marker == 0x00 || marker == 0xD8 {
            return Err(Error::InvalidMetadata("unexpected JPEG control marker"));
        }
        if marker == 0xD9 {
            if !seen_sos {
                return Err(Error::InvalidMetadata("JPEG missing SOS marker before EOI"));
            }
            from_sos.push(&bytes[marker_pair_start..pos]);
            seen_eoi = true;
            break;
        }
        if matches!(marker, 0x01 | 0xD0..=0xD7) {
            let target = if seen_sos {
                &mut from_sos
            } else {
                &mut pre_sos
            };
            target.push(&bytes[marker_pair_start..pos]);
            continue;
        }

        let len_bytes = bytes
            .get(pos..pos + 2)
            .ok_or(Error::InvalidMetadata("truncated JPEG segment length"))?;
        let seg_len = usize::from(u16::from_be_bytes([len_bytes[0], len_bytes[1]]));
        if seg_len < 2 || pos + seg_len > bytes.len() {
            return Err(Error::InvalidMetadata("invalid JPEG segment length"));
        }
        let payload = &bytes[pos + 2..pos + seg_len];
        let full_seg = &bytes[marker_pair_start..pos + seg_len];
        pos += seg_len;

        if marker == 0xDA {
            seen_sos = true;
            from_sos.push(full_seg);
            let ecs_end = scan_jpeg_entropy_end(bytes, pos)?;
            if ecs_end > pos {
                from_sos.push(&bytes[pos..ecs_end]);
            }
            pos = ecs_end;
            continue;
        }

        if should_strip_jpeg_segment(marker, payload, edits.is_some(), policy, &mut existing_exif) {
            continue;
        }

        let target = if seen_sos {
            &mut from_sos
        } else {
            &mut pre_sos
        };
        target.push(full_seg);
    }

    if !seen_sos || !seen_eoi {
        return Err(Error::InvalidMetadata("incomplete JPEG entropy stream"));
    }

    let new_tiff = build_exif_tiff(
        existing_exif,
        TiffBuildOptions {
            policy,
            edits,
            preserve_orientation: true,
            pixel_dims: None,
        },
    );

    let mut out = Vec::with_capacity(bytes.len() + new_tiff.len() + 32);
    out.extend_from_slice(&[0xFF, 0xD8]);

    let mut segments = pre_sos.into_iter().peekable();
    if let Some(first) = segments.peek()
        && first.len() >= 2
        && first[1] == 0xE0
        && let Some(app0) = segments.next()
    {
        out.extend_from_slice(app0);
    }

    if !new_tiff.is_empty() {
        let app1_len = 2 + JPEG_EXIF_HEADER.len() + new_tiff.len();
        let Ok(app1_len_u16) = u16::try_from(app1_len) else {
            return Err(Error::InvalidMetadata("EXIF segment exceeds 64 KiB"));
        };
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&app1_len_u16.to_be_bytes());
        out.extend_from_slice(JPEG_EXIF_HEADER);
        out.extend_from_slice(&new_tiff);
    }

    for seg in segments {
        out.extend_from_slice(seg);
    }
    for slice in from_sos {
        out.extend_from_slice(slice);
    }
    Ok(out)
}

fn scan_jpeg_entropy_end(bytes: &[u8], start: usize) -> Result<usize, Error> {
    let mut idx = start;
    while idx < bytes.len() {
        if bytes[idx] != 0xFF {
            idx += 1;
            continue;
        }
        let Some(&next) = bytes.get(idx + 1) else {
            return Err(Error::InvalidMetadata("truncated JPEG scan stream"));
        };
        if next == 0x00 {
            idx += 2;
            continue;
        }
        let mut marker_pos = idx + 1;
        while marker_pos < bytes.len() && bytes[marker_pos] == 0xFF {
            marker_pos += 1;
        }
        let Some(&marker) = bytes.get(marker_pos) else {
            return Err(Error::InvalidMetadata("truncated JPEG scan stream"));
        };
        if marker == 0x00 {
            return Err(Error::InvalidMetadata(
                "malformed JPEG marker padding before 0x00",
            ));
        }
        if (0xD0..=0xD7).contains(&marker) {
            idx = marker_pos + 1;
        } else {
            return Ok(idx);
        }
    }
    Err(Error::InvalidMetadata("truncated JPEG scan before EOI"))
}

fn should_strip_jpeg_segment<'a>(
    marker: u8,
    payload: &'a [u8],
    has_edits: bool,
    policy: SanitizePolicy,
    existing_exif: &mut Option<&'a [u8]>,
) -> bool {
    if marker == 0xE1 && payload.starts_with(JPEG_EXIF_HEADER) {
        if existing_exif.is_none() {
            *existing_exif = Some(&payload[JPEG_EXIF_HEADER.len()..]);
        }
        return true;
    }
    if marker == 0xE1
        && (has_edits || policy == SanitizePolicy::StripPrivacy)
        && (payload.starts_with(b"http://ns.adobe.com/xap/1.0/\0")
            || payload.starts_with(b"http://ns.adobe.com/xmp/extension/\0"))
    {
        return true;
    }
    policy == SanitizePolicy::StripPrivacy && matches!(marker, 0xED | 0xFE)
}

struct PngChunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
    raw: &'a [u8],
}

fn read_png_chunk_unchecked<'a>(bytes: &'a [u8], pos: &mut usize) -> Option<PngChunk<'a>> {
    let start = *pos;
    let len_bytes: [u8; 4] = bytes.get(start..start + 4)?.try_into().ok()?;
    let len = u32::from_be_bytes(len_bytes) as usize;
    if len > MAX_PNG_CHUNK_LEN {
        return None;
    }
    let kind: [u8; 4] = bytes.get(start + 4..start + 8)?.try_into().ok()?;
    let data_end = start.checked_add(8)?.checked_add(len)?;
    let crc_end = data_end.checked_add(4)?;
    let raw = bytes.get(start..crc_end)?;
    *pos = crc_end;
    Some(PngChunk {
        kind,
        data: &bytes[start + 8..data_end],
        raw,
    })
}

fn read_png_chunk_validated<'a>(bytes: &'a [u8], pos: &mut usize) -> Result<PngChunk<'a>, Error> {
    let start = *pos;
    let len_bytes: [u8; 4] = bytes
        .get(start..start + 4)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(Error::InvalidMetadata("truncated PNG chunk length"))?;
    let len = u32::from_be_bytes(len_bytes) as usize;
    if len > MAX_PNG_CHUNK_LEN {
        return Err(Error::InvalidMetadata("PNG chunk length exceeds 2 GiB"));
    }
    let kind: [u8; 4] = bytes
        .get(start + 4..start + 8)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(Error::InvalidMetadata("truncated PNG chunk type"))?;
    let data_end = start
        .checked_add(8)
        .and_then(|off| off.checked_add(len))
        .ok_or(Error::InvalidMetadata("PNG chunk length overflow"))?;
    let crc_end = data_end
        .checked_add(4)
        .ok_or(Error::InvalidMetadata("PNG chunk CRC overflow"))?;
    let raw = bytes
        .get(start..crc_end)
        .ok_or(Error::InvalidMetadata("truncated PNG chunk payload"))?;
    let crc_bytes: [u8; 4] = raw[8 + len..12 + len]
        .try_into()
        .map_err(|_| Error::InvalidMetadata("truncated PNG chunk CRC"))?;
    let expected_crc = u32::from_be_bytes(crc_bytes);
    let actual_crc = svgz::crc32(&raw[4..8 + len]);
    if actual_crc != expected_crc {
        return Err(Error::InvalidMetadata("invalid PNG chunk CRC"));
    }
    *pos = crc_end;
    Ok(PngChunk {
        kind,
        data: &bytes[start + 8..data_end],
        raw,
    })
}

fn rewrite_png(
    bytes: &[u8],
    edits: Option<&EditableFields>,
    policy: SanitizePolicy,
) -> Result<Vec<u8>, Error> {
    if bytes.len() < 8 || bytes[..8] != PNG_SIG {
        return Err(Error::InvalidMetadata("missing PNG signature"));
    }

    let mut existing_exif: Option<&[u8]> = None;
    let mut header_chunk: Option<&[u8]> = None;
    let mut pre_idat: Vec<&[u8]> = Vec::new();
    let mut from_idat: Vec<&[u8]> = Vec::new();
    let mut seen_idat = false;
    let mut seen_iend = false;
    let mut pos = 8usize;

    while pos < bytes.len() {
        let chunk = read_png_chunk_validated(bytes, &mut pos)?;
        if chunk.kind == *b"IHDR" {
            if header_chunk.is_some() || seen_idat {
                return Err(Error::InvalidMetadata(
                    "duplicate or misplaced PNG IHDR chunk",
                ));
            }
            header_chunk = Some(chunk.raw);
            continue;
        }
        if chunk.kind == *b"eXIf" {
            existing_exif = Some(chunk.data);
            continue;
        }
        if matches!(&chunk.kind, b"tEXt" | b"iTXt" | b"zTXt") {
            let key = png_text_keyword(chunk.data);
            if policy == SanitizePolicy::StripPrivacy {
                if edits.is_some() || !matches!(key, Some("Title" | "Description")) {
                    continue;
                }
            } else if edits.is_some()
                && matches!(
                    key,
                    Some(
                        "Title"
                            | "Description"
                            | "Author"
                            | "Copyright"
                            | "XML:com.adobe.xmp"
                            | "Raw profile type xmp"
                    )
                )
            {
                continue;
            }
        }
        if chunk.kind == *b"tIME" && policy == SanitizePolicy::StripPrivacy {
            continue;
        }
        if chunk.kind == *b"IDAT" {
            seen_idat = true;
        }
        if seen_idat {
            from_idat.push(chunk.raw);
        } else {
            pre_idat.push(chunk.raw);
        }
        if chunk.kind == *b"IEND" {
            seen_iend = true;
            break;
        }
    }

    let header = header_chunk.ok_or(Error::InvalidMetadata("missing PNG IHDR chunk"))?;
    if !seen_idat {
        return Err(Error::InvalidMetadata("missing PNG IDAT chunk"));
    }
    if !seen_iend {
        return Err(Error::InvalidMetadata("missing PNG IEND chunk"));
    }

    let new_tiff = build_exif_tiff(
        existing_exif,
        TiffBuildOptions {
            policy,
            edits,
            preserve_orientation: true,
            pixel_dims: None,
        },
    );

    let mut out = Vec::with_capacity(bytes.len() + new_tiff.len() + 128);
    out.extend_from_slice(&PNG_SIG);
    out.extend_from_slice(header);
    for raw in pre_idat {
        out.extend_from_slice(raw);
    }

    if !new_tiff.is_empty() {
        write_png_chunk(&mut out, *b"eXIf", &new_tiff)?;
    }
    if let Some(fields) = edits {
        let mut pairs = vec![
            ("Title", fields.title.as_deref()),
            ("Description", fields.description.as_deref()),
        ];
        if policy != SanitizePolicy::StripPrivacy {
            pairs.push(("Author", fields.artist.as_deref()));
            pairs.push(("Copyright", fields.copyright.as_deref()));
        }
        for (keyword, value) in pairs {
            if let Some(trimmed) = value.map(str::trim).filter(|t| !t.is_empty()) {
                png::text_metadata::ITXtChunk::new(keyword, trimmed).encode(&mut out)?;
            }
        }
    }

    for raw in from_idat {
        out.extend_from_slice(raw);
    }
    Ok(out)
}

fn png_text_keyword(data: &[u8]) -> Option<&str> {
    let sep = data.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&data[..sep]).ok()
}

fn write_png_chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) -> Result<(), Error> {
    let Ok(len) = u32::try_from(data.len()) else {
        return Err(Error::InvalidMetadata("PNG chunk exceeds 4 GiB"));
    };
    if data.len() > MAX_PNG_CHUNK_LEN {
        return Err(Error::InvalidMetadata("PNG chunk exceeds 2 GiB"));
    }
    out.extend_from_slice(&len.to_be_bytes());
    let crc_start = out.len();
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let crc = svgz::crc32(&out[crc_start..]);
    out.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

fn rewrite_webp(
    bytes: &[u8],
    edits: Option<&EditableFields>,
    policy: SanitizePolicy,
) -> Result<Vec<u8>, Error> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return Err(Error::InvalidMetadata("invalid WebP RIFF header"));
    }

    let mut pos = 12usize;
    let mut vp8x_payload: Option<[u8; 10]> = None;
    let mut existing_exif: Option<&[u8]> = None;
    let mut xmp_chunk: Option<&[u8]> = None;
    let mut has_iccp = false;
    let mut has_alpha = false;
    let mut has_anim = false;
    let mut body_chunks: Vec<([u8; 4], &[u8])> = Vec::new();
    let mut unknown_chunks: Vec<([u8; 4], &[u8])> = Vec::new();
    let mut iccp_chunk: Option<&[u8]> = None;
    let mut inferred_dims: Option<(u32, u32, bool)> = None;

    while pos < bytes.len() {
        let header = bytes
            .get(pos..pos + 8)
            .ok_or(Error::InvalidMetadata("truncated WebP chunk header"))?;
        let fourcc = [header[0], header[1], header[2], header[3]];
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let data_start = pos + 8;
        let Some(data_end) = data_start.checked_add(size) else {
            return Err(Error::InvalidMetadata("WebP chunk size overflow"));
        };
        if data_end > bytes.len() {
            return Err(Error::InvalidMetadata("truncated WebP chunk"));
        }
        let pad = size % 2;
        let Some(next_pos) = data_end.checked_add(pad) else {
            return Err(Error::InvalidMetadata("WebP chunk padding overflow"));
        };
        if next_pos > bytes.len() {
            return Err(Error::InvalidMetadata(
                "missing WebP odd chunk padding byte",
            ));
        }
        let payload = &bytes[data_start..data_end];
        pos = next_pos;

        match &fourcc {
            b"VP8X" => {
                if payload.len() != 10 {
                    return Err(Error::InvalidMetadata("invalid WebP VP8X chunk size"));
                }
                let mut arr = [0u8; 10];
                arr.copy_from_slice(payload);
                vp8x_payload = Some(arr);
            }
            b"EXIF" => {
                existing_exif = Some(strip_exif_header(payload));
            }
            b"XMP " => {
                if edits.is_none() && policy != SanitizePolicy::StripPrivacy {
                    xmp_chunk = Some(payload);
                }
            }
            b"ICCP" => {
                has_iccp = true;
                iccp_chunk = Some(payload);
            }
            b"ANIM" => {
                has_anim = true;
                body_chunks.push((fourcc, payload));
            }
            b"ANMF" => {
                has_anim = true;
                body_chunks.push((fourcc, payload));
            }
            b"ALPH" => {
                has_alpha = true;
                body_chunks.push((fourcc, payload));
            }
            b"VP8 " => {
                inferred_dims = inferred_dims.or_else(|| parse_vp8_dims(payload));
                body_chunks.push((fourcc, payload));
            }
            b"VP8L" => {
                inferred_dims = inferred_dims.or_else(|| parse_vp8l_dims(payload));
                if let Some((_, _, alpha)) = inferred_dims {
                    has_alpha |= alpha;
                }
                body_chunks.push((fourcc, payload));
            }
            _ => {
                unknown_chunks.push((fourcc, payload));
            }
        }
    }

    let new_tiff = build_exif_tiff(
        existing_exif,
        TiffBuildOptions {
            policy,
            edits,
            preserve_orientation: true,
            pixel_dims: None,
        },
    );

    let has_xmp = xmp_chunk.is_some();
    let needs_extended = vp8x_payload.is_some()
        || !new_tiff.is_empty()
        || iccp_chunk.is_some()
        || has_xmp
        || has_anim;

    let mut riff_body = Vec::with_capacity(bytes.len() + new_tiff.len() + 32);
    riff_body.extend_from_slice(b"WEBP");

    if needs_extended {
        let mut vp8x = match vp8x_payload {
            Some(arr) => arr,
            None => {
                let (w, h, _) = inferred_dims.ok_or(Error::InvalidMetadata(
                    "cannot determine WebP canvas dimensions for VP8X header",
                ))?;
                encode_vp8x(w, h)?
            }
        };
        let mut flags = u32::from_le_bytes([vp8x[0], vp8x[1], vp8x[2], vp8x[3]]);
        // Clear ICCP/EXIF/XMP bits and recompute accurately from retained chunks.
        flags &= !((1 << 5) | (1 << 3) | (1 << 2));
        if has_iccp {
            flags |= 1 << 5;
        }
        if has_alpha {
            flags |= 1 << 4;
        }
        if !new_tiff.is_empty() {
            flags |= 1 << 3;
        }
        if has_xmp {
            flags |= 1 << 2;
        }
        if has_anim {
            flags |= 1 << 1;
        }
        vp8x[..4].copy_from_slice(&flags.to_le_bytes());
        write_riff_chunk(&mut riff_body, *b"VP8X", &vp8x)?;
    }

    if let Some(iccp) = iccp_chunk {
        write_riff_chunk(&mut riff_body, *b"ICCP", iccp)?;
    }
    for (fourcc, payload) in body_chunks {
        write_riff_chunk(&mut riff_body, fourcc, payload)?;
    }
    if !new_tiff.is_empty() {
        write_riff_chunk(&mut riff_body, *b"EXIF", &new_tiff)?;
    }
    if let Some(xmp) = xmp_chunk {
        write_riff_chunk(&mut riff_body, *b"XMP ", xmp)?;
    }
    for (fourcc, payload) in unknown_chunks {
        write_riff_chunk(&mut riff_body, fourcc, payload)?;
    }

    let Ok(riff_len) = u32::try_from(riff_body.len()) else {
        return Err(Error::InvalidMetadata("WebP RIFF body exceeds 4 GiB"));
    };
    let mut out = Vec::with_capacity(riff_body.len() + 8);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_len.to_le_bytes());
    out.extend_from_slice(&riff_body);
    Ok(out)
}

fn write_riff_chunk(out: &mut Vec<u8>, fourcc: [u8; 4], payload: &[u8]) -> Result<(), Error> {
    let Ok(len) = u32::try_from(payload.len()) else {
        return Err(Error::InvalidMetadata("WebP chunk exceeds 4 GiB"));
    };
    out.extend_from_slice(&fourcc);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
    Ok(())
}

fn encode_vp8x(width: u32, height: u32) -> Result<[u8; 10], Error> {
    if width == 0 || height == 0 || width > (1 << 24) || height > (1 << 24) {
        return Err(Error::InvalidMetadata("WebP dimensions out of VP8X range"));
    }
    let w = (width - 1).to_le_bytes();
    let h = (height - 1).to_le_bytes();
    Ok([0, 0, 0, 0, w[0], w[1], w[2], h[0], h[1], h[2]])
}

fn parse_vp8l_dims(payload: &[u8]) -> Option<(u32, u32, bool)> {
    if payload.len() < 5 || payload[0] != 0x2F {
        return None;
    }
    let bits = u32::from_le_bytes(payload[1..5].try_into().ok()?);
    let width = (bits & 0x3FFF) + 1;
    let height = ((bits >> 14) & 0x3FFF) + 1;
    let alpha = ((bits >> 28) & 1) != 0;
    Some((width, height, alpha))
}

fn parse_vp8_dims(payload: &[u8]) -> Option<(u32, u32, bool)> {
    if payload.len() < 10 || payload[3] != 0x9D || payload[4] != 0x01 || payload[5] != 0x2A {
        return None;
    }
    let width = u32::from(u16::from_le_bytes([payload[6], payload[7]]) & 0x3FFF);
    let height = u32::from(u16::from_le_bytes([payload[8], payload[9]]) & 0x3FFF);
    Some((width, height, false))
}
