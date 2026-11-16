use std::{fs, path::Path, sync::Arc};

use gds::service::{
    file_tags,
    image::{
        PixelRect,
        decode::DecodedImage,
        encode::{EncodeOptions, encode_rgba},
        io::SaveMode,
        metadata::{EditableFields, ExifSummary, MetadataBundle, SanitizePolicy, TechnicalFacts},
        source::{FileStamp, Format, MemoryOrigin, Origin},
    },
};
use image::{Rgba, RgbaImage};

use crate::entities::{
    document::{AnnotationId, AnnotationItem, AnnotationKind, DocumentState, RgbaColor},
    metadata::MetadataState,
    preferences::Preferences,
};

use super::{
    PixelSource, SavePlan, build_clipboard_plan, build_save_plan, execute_clipboard_blocking,
    execute_save_blocking,
};

fn doc_with_red_square(width: u32, height: u32) -> DocumentState {
    let rgba = RgbaImage::from_pixel(width, height, Rgba([220, 20, 20, 255]));
    let decoded = DecodedImage {
        origin: Origin::Memory {
            kind: MemoryOrigin::Clipboard,
            bytes: Arc::from([]),
            format_hint: Some(Format::Png),
        },
        format: Format::Png,
        rgba: Arc::new(rgba),
        metadata: MetadataBundle {
            technical: TechnicalFacts {
                format: Format::Png,
                width,
                height,
                file_bytes: Some(0),
                color_type: "Rgba8".to_owned(),
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
        },
        was_color_managed: false,
        is_preview_downscaled: false,
        source_bytes: Arc::from([]),
    };
    DocumentState::from_decoded(decoded)
}

#[test]
fn stale_render_cache_still_bakes_annotations_into_save_and_clipboard_exports() {
    let mut doc = doc_with_red_square(16, 16);
    // Simulate a stale cached composited buffer at revision 0.
    doc.cache.composited = Some(Arc::clone(&doc.decoded.rgba));
    doc.cache.composited_revision = 0;

    // Now apply a bright blue rectangle annotation without waiting for RenderCache update.
    doc.annotations.push(AnnotationItem {
        id: AnnotationId(1),
        kind: AnnotationKind::Rect {
            rect: PixelRect::new(0, 0, 16, 16),
            stroke: RgbaColor::BLUE,
            stroke_width: 4.0,
            fill: Some(RgbaColor::BLUE),
        },
    });
    doc.bump_revision();
    assert_eq!(doc.edit_revision, 1);

    let meta = MetadataState::from_bundle(doc.decoded.metadata.clone(), Vec::new());
    let prefs = Preferences::default();

    let clip_plan = build_clipboard_plan(&doc, Some(&meta), &prefs);
    assert!(matches!(
        clip_plan.pixels,
        PixelSource::NeedsComposite { .. }
    ));
    let png_bytes = execute_clipboard_blocking(clip_plan).expect("clipboard PNG encode");
    let decoded_clip = image::load_from_memory(&png_bytes)
        .expect("decode clipboard PNG")
        .to_rgba8();
    assert_eq!(*decoded_clip.get_pixel(8, 8), Rgba([59, 130, 246, 255]));

    let dir = tempfile::tempdir().expect("tempdir");
    let out_path = dir.path().join("redacted.png");
    let save_plan = build_save_plan(out_path.clone(), SaveMode::SaveAs, &doc, &meta, &prefs);
    assert!(matches!(
        save_plan,
        SavePlan::PixelEncodeSave {
            pixels: PixelSource::NeedsComposite { .. },
            ..
        }
    ));
    let outcome = execute_save_blocking(save_plan).expect("save PNG");
    let decoded_saved = image::open(&out_path).expect("reopen saved PNG").to_rgba8();
    assert_eq!(*decoded_saved.get_pixel(8, 8), Rgba([59, 130, 246, 255]));
    assert!(outcome.composited_rgba.is_some());
    assert!(outcome.saved_tags.is_empty());
}

fn local_png_doc(path: &Path, tags: Vec<String>) -> (DocumentState, MetadataState) {
    let rgba = RgbaImage::from_pixel(8, 8, Rgba([20, 180, 90, 255]));
    let source_bytes = encode_rgba(
        &rgba,
        &EncodeOptions {
            format: Format::Png,
            jpeg_quality: 92,
            png_fast: true,
            existing_exif: None,
            metadata_edits: None,
            sanitize_policy: SanitizePolicy::KeepSafeOnPixelExport,
            jpeg_matte: [255, 255, 255],
        },
    )
    .expect("encode PNG");
    fs::write(path, &source_bytes).expect("write PNG");
    let stamp = FileStamp::capture(path).expect("stamp");
    let bundle = MetadataBundle {
        technical: TechnicalFacts {
            format: Format::Png,
            width: 8,
            height: 8,
            file_bytes: Some(source_bytes.len() as u64),
            color_type: "Rgba8".to_owned(),
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
    };
    let decoded = DecodedImage {
        origin: Origin::LocalFile {
            path: path.to_path_buf(),
            stamp: Some(stamp),
        },
        format: Format::Png,
        rgba: Arc::new(rgba),
        metadata: bundle.clone(),
        was_color_managed: false,
        is_preview_downscaled: false,
        source_bytes: Arc::from(source_bytes),
    };
    (
        DocumentState::from_decoded(decoded),
        MetadataState::from_bundle(bundle, tags),
    )
}

#[test]
fn overwrite_and_save_as_preserve_and_sync_user_xdg_tags() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src_path = dir.path().join("source.png");
    let initial_tags = vec!["keep".to_owned(), "old".to_owned()];
    let (mut doc, mut meta) = local_png_doc(&src_path, initial_tags.clone());
    let xattr_supported = file_tags::write(&src_path, &initial_tags).is_ok();
    if !xattr_supported {
        // On non-xattr temp filesystems, unchanged overwrite must still succeed cleanly.
        let plan = build_save_plan(
            src_path.clone(),
            SaveMode::Overwrite { expected: None },
            &doc,
            &meta,
            &Preferences::default(),
        );
        let outcome = execute_save_blocking(plan).expect("save without xattr support");
        assert!(outcome.saved_tags.is_empty());
        return;
    }

    // Overwrite (`Ctrl+S`) preserves unrelated external tags while applying user tag deltas.
    file_tags::change(&src_path, "external", true).expect("add external tag");
    meta.tags = vec!["keep".to_owned(), "new".to_owned()];
    meta.tags_dirty = true;
    let overwrite_plan = build_save_plan(
        src_path.clone(),
        SaveMode::Overwrite { expected: None },
        &doc,
        &meta,
        &Preferences::default(),
    );
    let overwrite_outcome = execute_save_blocking(overwrite_plan).expect("overwrite PNG");
    let expected_src_tags = vec!["keep".to_owned(), "external".to_owned(), "new".to_owned()];
    assert_eq!(overwrite_outcome.saved_tags, expected_src_tags);
    assert_eq!(
        file_tags::read(&src_path).expect("read overwritten tags"),
        expected_src_tags
    );

    // `Save As` with dirty tags writes `edited_tags` (retaining kept tags even if in `original_tags`).
    if let Origin::LocalFile { stamp, .. } = &mut doc.decoded.origin {
        *stamp = Some(overwrite_outcome.stamp);
    }
    let save_as_dirty = dir.path().join("save_as_dirty.png");
    let dirty_plan = build_save_plan(
        save_as_dirty.clone(),
        SaveMode::SaveAs,
        &doc,
        &meta,
        &Preferences::default(),
    );
    let dirty_outcome = execute_save_blocking(dirty_plan).expect("save as dirty tags");
    assert_eq!(dirty_outcome.saved_tags, vec!["keep", "new"]);
    assert_eq!(
        file_tags::read(&save_as_dirty).expect("read save_as_dirty tags"),
        vec!["keep", "new"]
    );

    // `Save As` with untouched non-empty tags copies them onto the destination file.
    meta.original_tags = meta.tags.clone();
    meta.tags_dirty = false;
    let save_as_copy = dir.path().join("save_as_copy.png");
    let copy_plan = build_save_plan(
        save_as_copy.clone(),
        SaveMode::SaveAs,
        &doc,
        &meta,
        &Preferences::default(),
    );
    let copy_outcome = execute_save_blocking(copy_plan).expect("save as untouched tags");
    assert_eq!(copy_outcome.saved_tags, vec!["keep", "new"]);
    assert_eq!(
        file_tags::read(&save_as_copy).expect("read save_as_copy tags"),
        vec!["keep", "new"]
    );
}
