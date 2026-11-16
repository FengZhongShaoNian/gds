use std::{cmp::Ordering, path::PathBuf};

use super::{compare_natural, compare_paths};

#[test]
fn natural_sort_orders_numeric_tokens_and_case_insensitively() {
    let mut names = vec![
        "img_10.png",
        "IMG_2.png",
        "img_1.png",
        "img_02.png",
        "frame-100.jpg",
        "frame-20.jpg",
    ];
    names.sort_by(|a, b| compare_natural(a, b));
    assert_eq!(
        names,
        vec![
            "frame-20.jpg",
            "frame-100.jpg",
            "img_1.png",
            "IMG_2.png",
            "img_02.png",
            "img_10.png",
        ]
    );
}

#[test]
fn natural_sort_handles_unicode_case_expansion_and_huge_digits() {
    assert_eq!(
        compare_natural("Übersicht_2.png", "übersicht_10.png"),
        Ordering::Less
    );
    assert_eq!(compare_natural("İ2.png", "i\u{307}10.png"), Ordering::Less);
    assert_eq!(
        compare_natural(
            "frame_999999999999999999999999999999.png",
            "frame_1000000000000000000000000000000.png"
        ),
        Ordering::Less
    );
}

#[test]
fn natural_sort_paths_is_antisymmetric_and_ignores_parent_order_first() {
    let mut paths = vec![
        PathBuf::from("/a/photo_12.png"),
        PathBuf::from("/z/photo_2.png"),
        PathBuf::from("/b/photo_100.png"),
    ];
    paths.sort_by(|a, b| compare_paths(a, b));
    assert_eq!(
        paths,
        vec![
            PathBuf::from("/z/photo_2.png"),
            PathBuf::from("/a/photo_12.png"),
            PathBuf::from("/b/photo_100.png"),
        ]
    );
}
