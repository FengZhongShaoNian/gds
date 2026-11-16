//! Pixel channel ordering (Straight RGBA8 <-> Straight BGRA8 for GPUI) and ICC -> sRGB CMS.

use super::Error;

/// Swaps red and blue channels in place for GPUI's `RenderImage` (`BGRA8`, non-premultiplied).
pub fn rgba_to_bgra_in_place(image: &mut image::RgbaImage) {
    let (pixels, _) = image.as_chunks_mut::<4>();
    for px in pixels {
        px.swap(0, 2);
    }
}

/// Clones an `RgbaImage` and converts it to straight `BGRA8`.
pub fn clone_as_bgra(image: &image::RgbaImage) -> image::RgbaImage {
    let mut cloned = image.clone();
    rgba_to_bgra_in_place(&mut cloned);
    cloned
}

/// Extracts a human-readable ICC color profile description if the profile is valid.
pub fn icc_profile_name(icc_bytes: &[u8]) -> Option<String> {
    let profile = moxcms::ColorProfile::new_from_slice(icc_bytes).ok()?;
    profile.description.as_ref().and_then(|desc| match desc {
        moxcms::ProfileText::PlainString(text) => non_empty(text),
        moxcms::ProfileText::Localizable(entries) => {
            entries.iter().find_map(|entry| non_empty(&entry.value))
        }
        moxcms::ProfileText::Description(info) => non_empty(&info.unicode_string)
            .or_else(|| non_empty(&info.ascii_string))
            .or_else(|| non_empty(&info.mac_string)),
    })
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Converts a decoded Straight RGBA8 buffer tagged with `icc_bytes` into standard sRGB.
///
/// Returns `Ok(true)` if a transform was applied, or `Ok(false)` when `icc_bytes` is empty.
///
/// # Errors
/// Returns [`Error::ColorTransform`] if the embedded ICC profile is malformed or incompatible
/// with 8-bit RGBA layout.
pub fn normalize_icc_to_srgb(
    image: &mut image::RgbaImage,
    icc_bytes: &[u8],
) -> Result<bool, Error> {
    if icc_bytes.is_empty() {
        return Ok(false);
    }
    let src_profile = moxcms::ColorProfile::new_from_slice(icc_bytes)
        .map_err(|err| Error::ColorTransform(err.to_string()))?;
    let dst_profile = moxcms::ColorProfile::new_srgb();
    let transform = src_profile
        .create_in_place_transform_8bit(
            moxcms::Layout::Rgba,
            &dst_profile,
            moxcms::TransformOptions::default(),
        )
        .map_err(|err| Error::ColorTransform(err.to_string()))?;

    transform
        .transform(image.as_mut())
        .map_err(|err| Error::ColorTransform(err.to_string()))?;
    Ok(true)
}
