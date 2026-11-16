use std::fs;

use super::{Preferences, read_startup_from};

#[test]
fn corrupt_toml_returns_default_and_notice_without_overwriting_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("settings.toml");
    let corrupt = "default_jpeg_quality = [broken\n";
    fs::write(&path, corrupt).expect("write corrupt config");

    let loaded = read_startup_from(path.clone()).expect("corrupt config degrades cleanly");
    assert_eq!(loaded.preferences, Preferences::default());
    assert!(loaded.notice.is_some());
    assert_eq!(
        fs::read_to_string(&path).expect("re-read disk config"),
        corrupt
    );
}

#[test]
fn missing_file_returns_defaults_without_creating_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("missing.toml");

    let mut loaded = read_startup_from(path.clone()).expect("missing file");
    assert_eq!(loaded.preferences, Preferences::default());
    assert!(loaded.notice.is_none());
    assert!(!path.exists());

    loaded.preferences.checkerboard_backdrop = false;
    loaded
        .store
        .save(&loaded.preferences)
        .expect("save new config");
    assert!(path.exists());
}
