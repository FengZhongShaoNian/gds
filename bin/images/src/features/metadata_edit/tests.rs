use std::{fs, path::PathBuf, sync::Arc};

use gds::service::{
    file_tags,
    image::{
        Error,
        encode::{EncodeOptions, encode_rgba},
        metadata::{EditableFields, SanitizePolicy, inspect_bytes},
        source::{FileStamp, Format},
    },
};
use image::{Rgba, RgbaImage};

use super::{MetadataCommitRequest, commit_metadata_and_tags_blocking};

fn encode_test_png() -> Vec<u8> {
    let img = RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255]));
    encode_rgba(
        &img,
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
    .expect("encode test PNG")
}

#[test]
fn commit_metadata_rejects_non_rewritable_formats_instead_of_dropping_edits() {
    let edits = EditableFields {
        title: Some("Should fail".to_owned()),
        ..EditableFields::default()
    };

    for format in [Format::Bmp, Format::Gif, Format::Qoi, Format::Svg] {
        let req = MetadataCommitRequest {
            path: PathBuf::from("/tmp/nonexistent.img"),
            expected_stamp: None,
            format,
            source_bytes: Arc::from([]),
            edits: Some(edits.clone()),
            strip_privacy: false,
            original_tags: Vec::new(),
            edited_tags: Vec::new(),
        };
        assert!(
            matches!(
                commit_metadata_and_tags_blocking(req),
                Err(Error::UnsupportedOperation(_))
            ),
            "{format:?} should reject lossless metadata edits"
        );

        let strip_req = MetadataCommitRequest {
            path: PathBuf::from("/tmp/nonexistent.img"),
            expected_stamp: None,
            format,
            source_bytes: Arc::from([]),
            edits: None,
            strip_privacy: true,
            original_tags: Vec::new(),
            edited_tags: Vec::new(),
        };
        assert!(
            matches!(
                commit_metadata_and_tags_blocking(strip_req),
                Err(Error::UnsupportedOperation(_))
            ),
            "{format:?} should reject lossless privacy strip"
        );
    }
}

#[test]
fn commit_metadata_returns_rewritten_bytes_and_skips_save_tags_when_unchanged() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("photo.png");
    let initial_bytes = encode_test_png();
    fs::write(&path, &initial_bytes).expect("write PNG");
    let stamp = FileStamp::capture(&path).expect("capture stamp");

    let req = MetadataCommitRequest {
        path: path.clone(),
        expected_stamp: Some(stamp),
        format: Format::Png,
        source_bytes: Arc::from(initial_bytes.clone()),
        edits: Some(EditableFields {
            title: Some("Sunset".to_owned()),
            ..EditableFields::default()
        }),
        strip_privacy: false,
        original_tags: Vec::new(),
        edited_tags: Vec::new(),
    };
    let outcome = commit_metadata_and_tags_blocking(req).expect("commit PNG metadata");
    assert!(outcome.updated_stamp.is_some());
    let rewritten = outcome.rewritten_bytes.expect("rewritten bytes returned");
    let bundle = inspect_bytes(&rewritten, Format::Png, Some(rewritten.len() as u64))
        .expect("inspect rewritten PNG");
    assert_eq!(bundle.editable.title.as_deref(), Some("Sunset"));

    // A symlink fails `file_tags::apply_edit`, so succeeding when `original_tags == edited_tags`
    // confirms unchanged tags bypass `save_tags` (matching `tmpfs`/FAT32 EOPNOTSUPP prevention).
    let link = dir.path().join("alias.png");
    std::os::unix::fs::symlink(&path, &link).expect("symlink");
    let unchanged_req = MetadataCommitRequest {
        path: link,
        expected_stamp: None,
        format: Format::Png,
        source_bytes: rewritten,
        edits: None,
        strip_privacy: false,
        original_tags: vec!["existing".to_owned()],
        edited_tags: vec!["existing".to_owned()],
    };
    let unchanged = commit_metadata_and_tags_blocking(unchanged_req)
        .expect("unchanged tags must not call save_tags");
    assert!(unchanged.updated_stamp.is_none());
    assert!(unchanged.rewritten_bytes.is_none());

    // Tag-only commits work on non-rewritable formats when tags differ and xattrs are supported.
    let bmp_path = dir.path().join("tag_only.bmp");
    fs::write(&bmp_path, b"BM").expect("write stub bmp");
    if file_tags::write(&bmp_path, &["initial".to_owned()]).is_ok() {
        let tag_only = commit_metadata_and_tags_blocking(MetadataCommitRequest {
            path: bmp_path.clone(),
            expected_stamp: None,
            format: Format::Bmp,
            source_bytes: Arc::from(&b"BM"[..]),
            edits: None,
            strip_privacy: false,
            original_tags: vec!["initial".to_owned()],
            edited_tags: vec!["initial".to_owned(), "desktop".to_owned()],
        })
        .expect("tag-only commit on BMP");
        assert!(tag_only.rewritten_bytes.is_none());
        assert_eq!(
            tag_only.updated_tags,
            vec!["initial".to_owned(), "desktop".to_owned()]
        );
        assert_eq!(
            file_tags::read(&bmp_path).expect("read bmp tags"),
            vec!["initial".to_owned(), "desktop".to_owned()]
        );
    }
}
