//! Atomic image saving with optimistic FileStamp concurrency checks and xattr preservation.
//!
//! Saves rely solely on same-filesystem temporary files renamed atomically over the target,
//! avoiding `.lock` sidecar files in photo folders.

#[cfg(test)]
mod tests;

use std::{fs, io::Write as _, path::Path};

use super::{Error, source::FileStamp};

/// Write policy controlling whether an existing destination may be overwritten.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveMode {
    /// Overwrites an existing file only if no external writer changed its [`FileStamp`].
    Overwrite { expected: Option<FileStamp> },
    /// Writes to a new user-selected destination (or replaces it intentionally).
    SaveAs,
}

/// Atomically writes `bytes` to `path` via a same-directory temporary file rename.
///
/// Rejects symbolic links and read-only destinations, detects external modifications via
/// [`FileStamp`] in [`SaveMode::Overwrite`], and preserves permissions and Linux
/// `user.xdg.tags` extended attributes across atomic replacement.
///
/// # Errors
/// Returns [`Error::Symlink`], [`Error::ReadOnly`], [`Error::ExternalConflict`], or [`Error::Io`].
pub fn atomic_save(path: &Path, bytes: &[u8], mode: SaveMode) -> Result<FileStamp, Error> {
    let parent = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    if let Some(meta) = inspect_existing(path)? {
        validate_destination_meta(path, &meta, &mode)?;
    } else if let SaveMode::Overwrite { expected: Some(_) } = mode {
        return Err(Error::ExternalConflict(path.to_path_buf()));
    }

    let existing = open_existing_no_follow(path)?;
    if let (
        Some((file, meta)),
        SaveMode::Overwrite {
            expected: Some(exp),
        },
    ) = (&existing, &mode)
    {
        let actual = FileStamp::from_metadata(path, meta)?;
        if actual != *exp {
            return Err(Error::ExternalConflict(path.to_path_buf()));
        }
        let _ = file;
    }

    let mut temp = create_umask_temp(parent, path)?;
    temp.write_all(bytes)
        .map_err(|source| Error::io(Some(path.to_path_buf()), source))?;

    if let Some((source_file, source_meta)) = &existing {
        temp.as_file()
            .set_permissions(source_meta.permissions())
            .map_err(|source| Error::io(Some(path.to_path_buf()), source))?;
        #[cfg(target_os = "linux")]
        crate::service::file_tags::copy_from(source_file, temp.path())
            .map_err(|source| Error::io(Some(path.to_path_buf()), source))?;
        #[cfg(not(target_os = "linux"))]
        let _ = source_file;
    }

    temp.as_file()
        .sync_all()
        .map_err(|source| Error::io(Some(path.to_path_buf()), source))?;

    if let Some(meta) = inspect_existing(path)? {
        validate_destination_meta(path, &meta, &mode)?;
    } else if let SaveMode::Overwrite { expected: Some(_) } = mode {
        return Err(Error::ExternalConflict(path.to_path_buf()));
    }

    temp.persist(path)
        .map_err(|err| Error::io(Some(path.to_path_buf()), err.error))?;

    if let Ok(dir) = fs::File::open(parent) {
        let _ = dir.sync_all();
    }

    FileStamp::capture(path)
}

fn inspect_existing(path: &Path) -> Result<Option<fs::Metadata>, Error> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::io(Some(path.to_path_buf()), err)),
    }
}

fn validate_destination_meta(
    path: &Path,
    meta: &fs::Metadata,
    mode: &SaveMode,
) -> Result<(), Error> {
    if meta.file_type().is_symlink() {
        return Err(Error::Symlink(path.to_path_buf()));
    }
    if !meta.is_file() {
        return Err(Error::io(
            Some(path.to_path_buf()),
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "destination is not a regular file",
            ),
        ));
    }
    check_writable(path, meta)?;
    if let SaveMode::Overwrite {
        expected: Some(exp),
    } = mode
    {
        let current = FileStamp::from_metadata(path, meta)?;
        if current != *exp {
            return Err(Error::ExternalConflict(path.to_path_buf()));
        }
    }
    Ok(())
}

/// Opens an existing file for bounded reading: uses `O_NOFOLLOW` first to capture a zero-TOCTOU
/// [`FileStamp`] on regular files, falling back on `ELOOP` to following the symlink without a
/// [`FileStamp`] so symlinked regular images still open while FIFOs/non-regular targets reject.
pub(crate) fn open_for_read(
    path: &Path,
) -> Result<(fs::File, fs::Metadata, Option<FileStamp>), Error> {
    match open_regular_fd(path, false) {
        Ok(Some((file, meta))) => {
            let stamp = Some(FileStamp::from_metadata(path, &meta)?);
            Ok((file, meta, stamp))
        }
        Ok(None) => Err(Error::io(
            Some(path.to_path_buf()),
            std::io::Error::from(std::io::ErrorKind::NotFound),
        )),
        Err(Error::Symlink(_)) => {
            let Some((file, meta)) = open_regular_fd(path, true)? else {
                return Err(Error::io(
                    Some(path.to_path_buf()),
                    std::io::Error::from(std::io::ErrorKind::NotFound),
                ));
            };
            Ok((file, meta, None))
        }
        Err(err) => Err(err),
    }
}

/// Opens an existing non-symlink regular file without following symlinks or blocking on FIFOs.
pub(crate) fn open_existing_no_follow(
    path: &Path,
) -> Result<Option<(fs::File, fs::Metadata)>, Error> {
    open_regular_fd(path, false)
}

fn open_regular_fd(
    path: &Path,
    follow_symlinks: bool,
) -> Result<Option<(fs::File, fs::Metadata)>, Error> {
    let file = {
        #[cfg(target_os = "linux")]
        {
            use rustix::{
                fs::{CWD, Mode, OFlags, openat},
                io::Errno,
            };
            let mut flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK;
            if !follow_symlinks {
                flags |= OFlags::NOFOLLOW;
            }
            match openat(CWD, path, flags, Mode::empty()) {
                Ok(fd) => fs::File::from(fd),
                Err(Errno::NOENT) => return Ok(None),
                Err(Errno::LOOP) if !follow_symlinks => {
                    return Err(Error::Symlink(path.to_path_buf()));
                }
                Err(err) => return Err(Error::io(Some(path.to_path_buf()), err.into())),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let mut options = fs::OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                let mut flags = nix::libc::O_CLOEXEC | nix::libc::O_NONBLOCK;
                if !follow_symlinks {
                    flags |= nix::libc::O_NOFOLLOW;
                }
                options.custom_flags(flags);
            }
            match options.open(path) {
                Ok(file) => file,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                #[cfg(unix)]
                Err(err) if !follow_symlinks && err.raw_os_error() == Some(nix::libc::ELOOP) => {
                    return Err(Error::Symlink(path.to_path_buf()));
                }
                Err(err) => return Err(Error::io(Some(path.to_path_buf()), err)),
            }
        }
    };

    let meta = file
        .metadata()
        .map_err(|source| Error::io(Some(path.to_path_buf()), source))?;
    if !meta.is_file() {
        return Err(Error::io(
            Some(path.to_path_buf()),
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "target is not a regular file",
            ),
        ));
    }
    Ok(Some((file, meta)))
}

fn check_writable(path: &Path, metadata: &fs::Metadata) -> Result<(), Error> {
    let readonly = metadata.permissions().readonly();
    #[cfg(unix)]
    let readonly = {
        use std::os::unix::fs::PermissionsExt as _;
        // Check mode bits as well as `access(W_OK)` so root-owned test runners
        // still reject `0o444` fixtures while non-root users detect root-owned
        // `0o644` files and `EROFS`/ACL mounts.
        if readonly || (metadata.permissions().mode() & 0o222) == 0 {
            true
        } else {
            match nix::unistd::access(path, nix::unistd::AccessFlags::W_OK) {
                Ok(()) => false,
                Err(nix::errno::Errno::EACCES | nix::errno::Errno::EROFS) => true,
                Err(err) => return Err(Error::io(Some(path.to_path_buf()), err.into())),
            }
        }
    };
    if readonly {
        return Err(Error::ReadOnly(path.to_path_buf()));
    }
    Ok(())
}

fn create_umask_temp(parent: &Path, target: &Path) -> Result<tempfile::NamedTempFile, Error> {
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        builder.permissions(fs::Permissions::from_mode(0o666));
    }
    builder
        .tempfile_in(parent)
        .map_err(|source| Error::io(Some(target.to_path_buf()), source))
}
