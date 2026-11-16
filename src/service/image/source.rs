//! Input origin identity, container format classification, and capability matrix.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use super::Error;

const SVG_SNIFF_BYTES: usize = 4096;

/// Supported filename extensions (lowercase ASCII without leading dot).
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jpe", "webp", "gif", "svg", "svgz", "bmp", "dib", "ico", "cur", "tif",
    "tiff", "qoi", "tga", "icb", "vda", "vst", "pbm", "pgm", "ppm", "pam", "pnm",
];

/// Supported clipboard and desktop MIME types in preference order.
pub const MIME_TYPES: &[&str] = &[
    "image/png",
    "image/webp",
    "image/jpeg",
    "image/gif",
    "image/svg+xml",
    "image/bmp",
    "image/tiff",
    "image/x-icon",
    "image/qoi",
    "image/x-tga",
    "image/x-portable-anymap",
];

/// Lightweight filesystem snapshot used to detect concurrent external edits without
/// re-reading entire image payloads or dropping `.lock` sidecar files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub size: u64,
    pub modified: Option<SystemTime>,
    #[cfg(unix)]
    pub dev: u64,
    #[cfg(unix)]
    pub ino: u64,
    #[cfg(unix)]
    pub mtime: i64,
    #[cfg(unix)]
    pub mtime_nsec: i64,
}

impl FileStamp {
    /// Captures a stamp from `path` without following a terminal symbolic link.
    ///
    /// # Errors
    /// Returns [`Error::Symlink`] when `path` is a symlink or I/O errors from `lstat`.
    pub fn capture(path: &Path) -> Result<Self, Error> {
        let metadata =
            fs::symlink_metadata(path).map_err(|source| Error::io(Some(path.into()), source))?;
        Self::from_metadata(path, &metadata)
    }

    /// Builds a stamp from already-fetched `symlink_metadata`.
    ///
    /// # Errors
    /// Returns [`Error::Symlink`] or `InvalidInput` when `metadata` is not a regular file.
    pub fn from_metadata(path: &Path, metadata: &fs::Metadata) -> Result<Self, Error> {
        if metadata.file_type().is_symlink() {
            return Err(Error::Symlink(path.into()));
        }
        if !metadata.is_file() {
            return Err(Error::io(
                Some(path.into()),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "image target is not a regular file",
                ),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(Self {
                size: metadata.len(),
                modified: metadata.modified().ok(),
                dev: metadata.dev(),
                ino: metadata.ino(),
                mtime: metadata.mtime(),
                mtime_nsec: metadata.mtime_nsec(),
            })
        }
        #[cfg(not(unix))]
        {
            Ok(Self {
                size: metadata.len(),
                modified: metadata.modified().ok(),
            })
        }
    }
}

/// Origin category for in-memory image payloads without a primary disk path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryOrigin {
    Stdin,
    Clipboard,
}

/// Recognized image container and raster/vector format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Png,
    Jpeg,
    WebP,
    Gif,
    Svg,
    Bmp,
    Ico,
    Tiff,
    Qoi,
    Tga,
    Pnm,
}

/// Four-tier capability profile declaring what operations a format supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub can_probe_metadata: bool,
    pub can_decode_static: bool,
    pub can_play_animation: bool,
    pub can_encode: bool,
    pub can_losslessly_rewrite_metadata: bool,
}

impl Format {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        Self::from_extension(ext)
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        let lower = ext.trim_start_matches('.').to_ascii_lowercase();
        match lower.as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" | "jpe" => Some(Self::Jpeg),
            "webp" => Some(Self::WebP),
            "gif" => Some(Self::Gif),
            "svg" | "svgz" => Some(Self::Svg),
            "bmp" | "dib" => Some(Self::Bmp),
            "ico" | "cur" => Some(Self::Ico),
            "tif" | "tiff" => Some(Self::Tiff),
            "qoi" => Some(Self::Qoi),
            "tga" | "icb" | "vda" | "vst" => Some(Self::Tga),
            "pbm" | "pgm" | "ppm" | "pam" | "pnm" => Some(Self::Pnm),
            _ => None,
        }
    }

    pub fn from_mime(mime: &str) -> Option<Self> {
        let base = mime.split(';').next()?.trim().to_ascii_lowercase();
        match base.as_str() {
            "image/png" => Some(Self::Png),
            "image/jpeg" | "image/jpg" => Some(Self::Jpeg),
            "image/webp" => Some(Self::WebP),
            "image/gif" => Some(Self::Gif),
            "image/svg+xml" => Some(Self::Svg),
            "image/bmp" | "image/x-ms-bmp" => Some(Self::Bmp),
            "image/x-icon" | "image/vnd.microsoft.icon" => Some(Self::Ico),
            "image/tiff" => Some(Self::Tiff),
            "image/qoi" => Some(Self::Qoi),
            "image/x-tga" | "image/x-targa" => Some(Self::Tga),
            "image/x-portable-anymap"
            | "image/x-portable-bitmap"
            | "image/x-portable-graymap"
            | "image/x-portable-pixmap" => Some(Self::Pnm),
            _ => None,
        }
    }

    pub fn sniff(bytes: &[u8], path_hint: Option<&Path>) -> Option<Self> {
        if is_svg_bytes(bytes) {
            return Some(Self::Svg);
        }
        if let Ok(guessed) = image::guess_format(bytes)
            && let Some(fmt) = Self::from_image_format(guessed)
        {
            return Some(fmt);
        }
        path_hint.and_then(Self::from_path)
    }

    pub const fn from_image_format(format: image::ImageFormat) -> Option<Self> {
        match format {
            image::ImageFormat::Png => Some(Self::Png),
            image::ImageFormat::Jpeg => Some(Self::Jpeg),
            image::ImageFormat::WebP => Some(Self::WebP),
            image::ImageFormat::Gif => Some(Self::Gif),
            image::ImageFormat::Bmp => Some(Self::Bmp),
            image::ImageFormat::Ico => Some(Self::Ico),
            image::ImageFormat::Tiff => Some(Self::Tiff),
            image::ImageFormat::Qoi => Some(Self::Qoi),
            image::ImageFormat::Tga => Some(Self::Tga),
            image::ImageFormat::Pnm => Some(Self::Pnm),
            _ => None,
        }
    }

    pub const fn to_image_format(self) -> Option<image::ImageFormat> {
        match self {
            Self::Png => Some(image::ImageFormat::Png),
            Self::Jpeg => Some(image::ImageFormat::Jpeg),
            Self::WebP => Some(image::ImageFormat::WebP),
            Self::Gif => Some(image::ImageFormat::Gif),
            Self::Svg => None,
            Self::Bmp => Some(image::ImageFormat::Bmp),
            Self::Ico => Some(image::ImageFormat::Ico),
            Self::Tiff => Some(image::ImageFormat::Tiff),
            Self::Qoi => Some(image::ImageFormat::Qoi),
            Self::Tga => Some(image::ImageFormat::Tga),
            Self::Pnm => Some(image::ImageFormat::Pnm),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::WebP => "WebP",
            Self::Gif => "GIF",
            Self::Svg => "SVG",
            Self::Bmp => "BMP",
            Self::Ico => "ICO",
            Self::Tiff => "TIFF",
            Self::Qoi => "QOI",
            Self::Tga => "TGA",
            Self::Pnm => "PNM",
        }
    }

    pub const fn default_extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Gif => "gif",
            Self::Svg => "svg",
            Self::Bmp => "bmp",
            Self::Ico => "ico",
            Self::Tiff => "tiff",
            Self::Qoi => "qoi",
            Self::Tga => "tga",
            Self::Pnm => "ppm",
        }
    }

    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::WebP => "image/webp",
            Self::Gif => "image/gif",
            Self::Svg => "image/svg+xml",
            Self::Bmp => "image/bmp",
            Self::Ico => "image/x-icon",
            Self::Tiff => "image/tiff",
            Self::Qoi => "image/qoi",
            Self::Tga => "image/x-tga",
            Self::Pnm => "image/x-portable-anymap",
        }
    }

    pub const fn capabilities(self) -> Capabilities {
        match self {
            Self::Png | Self::Jpeg => Capabilities {
                can_probe_metadata: true,
                can_decode_static: true,
                can_play_animation: false,
                can_encode: true,
                can_losslessly_rewrite_metadata: true,
            },
            Self::WebP => Capabilities {
                can_probe_metadata: true,
                can_decode_static: true,
                can_play_animation: true,
                can_encode: true,
                can_losslessly_rewrite_metadata: true,
            },
            Self::Gif => Capabilities {
                can_probe_metadata: true,
                can_decode_static: true,
                can_play_animation: true,
                can_encode: false,
                can_losslessly_rewrite_metadata: false,
            },
            Self::Bmp | Self::Tiff | Self::Qoi => Capabilities {
                can_probe_metadata: true,
                can_decode_static: true,
                can_play_animation: false,
                can_encode: true,
                can_losslessly_rewrite_metadata: false,
            },
            Self::Svg | Self::Ico | Self::Tga | Self::Pnm => Capabilities {
                can_probe_metadata: true,
                can_decode_static: true,
                can_play_animation: false,
                can_encode: false,
                can_losslessly_rewrite_metadata: false,
            },
        }
    }
}

/// Source identity of an image loaded by the viewer/editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    LocalFile {
        path: PathBuf,
        stamp: Option<FileStamp>,
    },
    Memory {
        kind: MemoryOrigin,
        bytes: Arc<[u8]>,
        format_hint: Option<Format>,
    },
}

impl Origin {
    pub fn local(path: PathBuf) -> Self {
        let stamp = FileStamp::capture(&path).ok();
        Self::LocalFile { path, stamp }
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::LocalFile { path, .. } => Some(path),
            Self::Memory { .. } => None,
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::LocalFile { path, .. } => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            Self::Memory {
                kind: MemoryOrigin::Stdin,
                ..
            } => "stdin".to_owned(),
            Self::Memory {
                kind: MemoryOrigin::Clipboard,
                ..
            } => "clipboard".to_owned(),
        }
    }
}

pub(crate) fn is_svg_bytes(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(SVG_SNIFF_BYTES)]);
    let head = head.trim_start_matches('\u{feff}').trim_start();
    head.starts_with("<svg")
        || ((head.starts_with("<?xml")
            || head.starts_with("<!DOCTYPE")
            || head.starts_with("<!--"))
            && head.contains("<svg"))
}
