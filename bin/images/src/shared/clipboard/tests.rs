use std::path::PathBuf;

use gds::service::image::source::Format;
use gpui_kit::{ClipboardMime, ClipboardMimeOffer};

use super::{
    GNOME_FILES, URI_LIST, extract_candidate_local_image_paths, first_raw_mime_format,
    local_file_uri, parse_gnome_files_uris,
};

#[test]
fn local_file_uri_rejects_credentials_query_fragment_and_relative_uris() {
    assert_eq!(
        local_file_uri("file:///tmp/photo.png"),
        Some(PathBuf::from("/tmp/photo.png"))
    );
    assert_eq!(
        local_file_uri("file://localhost/tmp/photo%201.jpg"),
        Some(PathBuf::from("/tmp/photo 1.jpg"))
    );

    assert!(local_file_uri("file:photo.png").is_none());
    assert!(local_file_uri("file://remote-host/tmp/photo.png").is_none());
    assert!(local_file_uri("file://user@localhost/tmp/photo.png").is_none());
    assert!(local_file_uri("file://user:pass@localhost/tmp/photo.png").is_none());
    assert!(local_file_uri("file://localhost:8080/tmp/photo.png").is_none());
    assert!(local_file_uri("file:///tmp/photo.png?raw=1").is_none());
    assert!(local_file_uri("file:///tmp/photo.png#section").is_none());
    assert!(local_file_uri("file:///tmp/photo\0.png").is_none());
    assert!(local_file_uri("https://example.com/photo.png").is_none());
}

#[test]
fn gnome_copied_files_requires_copy_or_cut_intent_header() {
    assert_eq!(
        parse_gnome_files_uris(b"copy\nfile:///tmp/a.png\n"),
        Some("file:///tmp/a.png\n")
    );
    assert_eq!(
        parse_gnome_files_uris(b"cut\r\nfile:///tmp/a.png"),
        Some("file:///tmp/a.png")
    );
    assert!(parse_gnome_files_uris(b"move\nfile:///tmp/a.png").is_none());
    assert!(parse_gnome_files_uris(b"file:///tmp/a.png").is_none());
}

#[test]
fn offer_extraction_filters_supported_formats_and_detects_actual_mime() {
    let offer = ClipboardMimeOffer {
        id: 1,
        entries: vec![
            ClipboardMime {
                mime_type: GNOME_FILES.into(),
                bytes: b"copy\nfile:///tmp/notes.txt\nfile:///tmp/pic.webp?bad=1\nfile:///tmp/pic.webp\n".to_vec(),
            },
            ClipboardMime {
                mime_type: URI_LIST.into(),
                bytes: b"file:///tmp/ignored.png".to_vec(),
            },
        ],
    };
    assert_eq!(
        extract_candidate_local_image_paths(&offer),
        vec![PathBuf::from("/tmp/pic.webp")]
    );

    let invalid_gnome = ClipboardMimeOffer {
        id: 2,
        entries: vec![
            ClipboardMime {
                mime_type: GNOME_FILES.into(),
                bytes: b"bogus\nfile:///tmp/a.png".to_vec(),
            },
            ClipboardMime {
                mime_type: URI_LIST.into(),
                bytes: b"file:///tmp/a.png".to_vec(),
            },
        ],
    };
    assert!(extract_candidate_local_image_paths(&invalid_gnome).is_empty());

    let raw_webp = ClipboardMimeOffer {
        id: 3,
        entries: vec![ClipboardMime {
            mime_type: "image/webp".into(),
            bytes: vec![1, 2, 3, 4],
        }],
    };
    assert_eq!(first_raw_mime_format(&raw_webp), Some(Format::WebP));
}
