use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::{SaveMode, atomic_save, open_existing_no_follow};
use crate::service::image::{Error, source::FileStamp};

fn temp_dir() -> TempDir {
    TempDir::new().expect("create temporary image directory")
}

fn has_lock_sidecar(dir: &Path) -> bool {
    fs::read_dir(dir)
        .expect("read test directory")
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().ends_with(".lock"))
}

#[test]
fn atomic_save_overwrites_without_creating_lock_sidecars() {
    let dir = temp_dir();
    let path = dir.path().join("photo.png");
    fs::write(&path, b"before").expect("seed file");
    let stamp = FileStamp::capture(&path).expect("capture initial stamp");

    let next_stamp = atomic_save(
        &path,
        b"after-edit",
        SaveMode::Overwrite {
            expected: Some(stamp.clone()),
        },
    )
    .expect("atomic overwrite");

    assert_eq!(fs::read(&path).expect("read updated file"), b"after-edit");
    assert!(!has_lock_sidecar(dir.path()));
    assert_ne!(next_stamp, stamp);
}

#[test]
fn atomic_save_detects_external_conflict_when_stamp_changes() {
    let dir = temp_dir();
    let path = dir.path().join("conflict.png");
    fs::write(&path, b"v1").expect("seed v1");
    let stale_stamp = FileStamp::capture(&path).expect("capture v1 stamp");

    fs::write(&path, b"external-v2-longer").expect("simulate external writer");

    let err = atomic_save(
        &path,
        b"my-overwrite",
        SaveMode::Overwrite {
            expected: Some(stale_stamp),
        },
    )
    .expect_err("external edit must conflict");
    assert!(matches!(err, Error::ExternalConflict(_)));
    assert_eq!(
        fs::read(&path).expect("external write intact"),
        b"external-v2-longer"
    );
}

#[cfg(unix)]
#[test]
fn atomic_save_refuses_symlinks_without_modifying_target() {
    let dir = temp_dir();
    let target = dir.path().join("real.png");
    let link = dir.path().join("link.png");
    fs::write(&target, b"untouched").expect("write target");
    std::os::unix::fs::symlink(&target, &link).expect("create symlink");

    let err =
        atomic_save(&link, b"new-data", SaveMode::SaveAs).expect_err("symlink save must fail");
    assert!(matches!(err, Error::Symlink(_)));
    assert_eq!(fs::read(&target).expect("target intact"), b"untouched");
}

#[test]
fn atomic_save_refuses_readonly_destination() {
    let dir = temp_dir();
    let path = dir.path().join("readonly.png");
    fs::write(&path, b"locked").expect("write file");
    let mut perms = fs::metadata(&path).expect("stat file").permissions();
    perms.set_readonly(true);
    fs::set_permissions(&path, perms).expect("set readonly");

    let err =
        atomic_save(&path, b"attempt", SaveMode::SaveAs).expect_err("readonly save must fail");
    assert!(matches!(err, Error::ReadOnly(_)));
    assert_eq!(fs::read(&path).expect("readonly content intact"), b"locked");
}

#[cfg(unix)]
#[test]
fn atomic_save_preserves_unix_permissions_and_xdg_tags() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = temp_dir();
    let path = dir.path().join("tagged.png");
    fs::write(&path, b"initial").expect("seed tagged file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("set mode 0640");

    #[cfg(target_os = "linux")]
    crate::service::file_tags::write(&path, &["travel".to_owned(), "favorite".to_owned()])
        .expect("write xdg tags");

    let stamp = FileStamp::capture(&path).expect("capture stamp");
    let _ = atomic_save(
        &path,
        b"updated-image-bytes",
        SaveMode::Overwrite {
            expected: Some(stamp),
        },
    )
    .expect("overwrite tagged file");

    let mode = fs::metadata(&path)
        .expect("stat saved")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o640);

    #[cfg(target_os = "linux")]
    assert_eq!(
        crate::service::file_tags::read(&path).expect("read preserved tags"),
        vec!["travel".to_owned(), "favorite".to_owned()]
    );
}

#[cfg(unix)]
#[test]
fn atomic_save_save_as_matches_default_file_mode_umask() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = temp_dir();
    let baseline = dir.path().join("baseline.bin");
    fs::write(&baseline, b"x").expect("write baseline");
    let expected_mode = fs::metadata(&baseline)
        .expect("stat baseline")
        .permissions()
        .mode()
        & 0o777;

    let saved = dir.path().join("export.png");
    let _ = atomic_save(&saved, b"png-bytes", SaveMode::SaveAs).expect("save as new file");
    let actual_mode = fs::metadata(&saved)
        .expect("stat saved")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(actual_mode, expected_mode);
}

#[cfg(unix)]
#[test]
fn open_existing_no_follow_rejects_fifo_without_blocking() {
    let dir = temp_dir();
    let fifo = dir.path().join("pipe.png");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::from_bits_truncate(0o600)).expect("mkfifo");

    let err = open_existing_no_follow(&fifo).expect_err("FIFO must be rejected non-blockingly");
    assert!(matches!(err, Error::Io { .. }));
}
