use std::{fs, io::ErrorKind, os::unix::fs::symlink, path::Path};

use tokio_util::sync::CancellationToken;

use super::{normalize_parent_dir, scan_siblings};

#[test]
fn bare_relative_filename_normalizes_parent_dir_to_dot() {
    assert_eq!(
        normalize_parent_dir(Path::new("photo.png")),
        Some(Path::new("."))
    );
    assert_eq!(
        normalize_parent_dir(Path::new("/tmp/photo.png")),
        Some(Path::new("/tmp"))
    );
    assert_eq!(normalize_parent_dir(Path::new("/")), None);
}

#[test]
fn cancelled_scan_returns_interrupted_error_instead_of_partial_catalog() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("a.png"), b"png").expect("write a");
    fs::write(dir.path().join("b.png"), b"png").expect("write b");

    let cancel = CancellationToken::new();
    cancel.cancel();

    let err = scan_siblings(&dir.path().join("a.png"), &cancel)
        .expect_err("cancelled scan must fail cleanly");
    assert_eq!(err.kind(), ErrorKind::Interrupted);
}

#[test]
fn symlinked_target_matches_underlying_file_by_device_and_inode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let real = dir.path().join("photo_2.png");
    let other = dir.path().join("photo_10.png");
    let alias_dir = tempfile::tempdir().expect("alias tempdir");
    let link = alias_dir.path().join("alias.png");

    fs::write(&real, b"real").expect("write real");
    fs::write(&other, b"other").expect("write other");
    symlink(&real, &link).expect("create symlink");

    let link_in_same_dir = dir.path().join("alias_1.png");
    symlink(&real, &link_in_same_dir).expect("create local symlink");

    // Non-normalized dot component forces fallback to (dev, ino) matching without canonicalize().
    let dot_target = dir.path().join(".").join("photo_2.png");
    let catalog = scan_siblings(&dot_target, &CancellationToken::new()).expect("scan siblings");
    let names: Vec<_> = catalog
        .entries
        .iter()
        .map(|e| e.meta.display_name.as_ref().to_owned())
        .collect();
    assert_eq!(names, vec!["alias_1.png", "photo_2.png", "photo_10.png"]);
    assert_eq!(catalog.current_index, Some(1));

    // Selecting the symlink itself must keep index 0 (`alias_1.png`), while selecting the target
    // via a non-normalized `..` traversal must prefer the real regular file (`photo_2.png`).
    let link_selected =
        scan_siblings(&link_in_same_dir, &CancellationToken::new()).expect("scan symlink");
    assert_eq!(link_selected.current_index, Some(0));

    let dir_name = dir.path().file_name().expect("tempdir name");
    let dotdot_target = dir.path().join("..").join(dir_name).join("photo_2.png");
    let dotdot_selected =
        scan_siblings(&dotdot_target, &CancellationToken::new()).expect("scan dotdot target");
    assert_eq!(dotdot_selected.current_index, Some(1));
}
