//! Extended-attribute desktop tag I/O delegated to `gds::service::file_tags`.

use std::{io, path::Path};

use gds::service::file_tags;

/// Reads Linux `user.xdg.tags` attributes for `path` (empty when absent or unsupported).
pub fn load_tags(path: &Path) -> Vec<String> {
    file_tags::read(path).unwrap_or_default()
}

/// Applies differential desktop tag edits safely onto `path`.
///
/// # Errors
/// Returns validation errors or filesystem extended-attribute errors from `file_tags`.
pub fn save_tags(path: &Path, original: &[String], edited: &[String]) -> io::Result<Vec<String>> {
    file_tags::apply_edit(path, original, edited)?;
    file_tags::read(path)
}
