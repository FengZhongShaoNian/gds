//! Wayland-native image and URI clipboard integration plus GPUI fallback.

#[cfg(test)]
mod tests;

use std::{path::PathBuf, sync::Arc};

use gds::{
    app::{AppExt as _, Context as GdsContext, Gds},
    service::{
        image::source::{Format, MIME_TYPES, MemoryOrigin, Origin},
        task_manager::Scope,
    },
};
use gpui_kit::{
    App, ClipboardAccessError, ClipboardEntry, ClipboardItem, ClipboardMime, ClipboardMimeOffer,
    Image, ImageFormat, Task,
};

const URI_LIST: &str = "text/uri-list";
const GNOME_FILES: &str = "x-special/gnome-copied-files";
const MAX_URI_LIST_BYTES: usize = 1024 * 1024;
const MAX_IMAGE_CLIPBOARD_BYTES: usize = 128 * 1024 * 1024;

/// Lightweight descriptor of what the clipboard currently holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardOfferKind {
    LocalImageFile(PathBuf),
    RawImageMime(Format),
}

/// Errors reported while reading or writing clipboard payloads.
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("clipboard is empty or does not contain a supported image")]
    Empty,
    #[error("clipboard MIME access error: {0}")]
    Mime(#[from] ClipboardAccessError),
    #[error("failed to read clipboard: {0}")]
    Read(#[from] gpui_kit::ClipboardReadError),
}

/// Scoped accessor exposing image clipboard operations on [`gds::app::Gds`].
#[derive(Clone, Copy)]
pub struct Accessor<C> {
    cx: C,
}

pub trait ClipboardExt {
    type Context<'a>: GdsContext
    where
        Self: 'a;

    fn clipboard(&mut self) -> Accessor<Self::Context<'_>>;
}

impl<C: GdsContext> ClipboardExt for Gds<C> {
    type Context<'a>
        = C::Reborrowed<'a>
    where
        C: 'a;

    fn clipboard(&mut self) -> Accessor<C::Reborrowed<'_>> {
        Accessor {
            cx: self.reborrow().into_inner(),
        }
    }
}

impl<C: std::ops::Deref<Target = App>> Accessor<C> {
    /// Probes the current clipboard selection without running filesystem I/O on the UI thread.
    pub fn probe_offer(&self) -> Task<Option<ClipboardOfferKind>> {
        let tasks = self.cx.gds().tasks().scope();
        if !self.cx.supports_clipboard_mime() {
            let read = self.cx.read_from_clipboard_async();
            return self.cx.spawn(async move |_| {
                let item = read.await.ok()??;
                offer_from_clipboard_item(&item, &tasks).await
            });
        }

        let read = self
            .cx
            .read_clipboard_mime(requested_mime_types(), MAX_IMAGE_CLIPBOARD_BYTES);
        self.cx.spawn(async move |_| {
            let offer = read.await.ok()??;
            offer_from_mime_offer(&offer, &tasks).await
        })
    }

    /// Reads an image [`Origin`] from the system clipboard, preferring valid local image URIs.
    pub fn read_origin(&self) -> Task<Result<Origin, ClipboardError>> {
        let tasks = self.cx.gds().tasks().scope();
        if !self.cx.supports_clipboard_mime() {
            let read = self.cx.read_from_clipboard_async();
            return self.cx.spawn(async move |_| {
                let item = read.await?.ok_or(ClipboardError::Empty)?;
                origin_from_clipboard_item(item, &tasks)
                    .await
                    .ok_or(ClipboardError::Empty)
            });
        }

        let read = self
            .cx
            .read_clipboard_mime(requested_mime_types(), MAX_IMAGE_CLIPBOARD_BYTES);
        self.cx.spawn(async move |_| {
            let Some(offer) = read.await? else {
                return Err(ClipboardError::Empty);
            };
            origin_from_mime_offer(offer, &tasks)
                .await
                .ok_or(ClipboardError::Empty)
        })
    }

    /// Publishes PNG-encoded image bytes to the system clipboard.
    ///
    /// # Errors
    /// Returns [`ClipboardError::Mime`] if Wayland MIME clipboard publication fails.
    pub fn write_png(&self, png_bytes: Vec<u8>) -> Result<(), ClipboardError> {
        if self.cx.supports_clipboard_mime() {
            self.cx.write_clipboard_mime(
                vec![ClipboardMime {
                    mime_type: Format::Png.mime_type().to_owned(),
                    bytes: png_bytes,
                }],
                None,
            )?;
        } else {
            let image = Arc::new(Image::from_bytes(ImageFormat::Png, png_bytes));
            self.cx
                .write_to_clipboard(ClipboardItem::new_image(image.as_ref()));
        }
        Ok(())
    }
}

fn requested_mime_types() -> Vec<String> {
    let mut types = Vec::with_capacity(2 + MIME_TYPES.len());
    types.push(GNOME_FILES.to_owned());
    types.push(URI_LIST.to_owned());
    types.extend(MIME_TYPES.iter().map(|mime| (*mime).to_owned()));
    types
}

async fn first_existing_file(candidates: Vec<PathBuf>, tasks: &Scope) -> Option<PathBuf> {
    if candidates.is_empty() {
        return None;
    }
    tasks
        .spawn_blocking_named("clipboard-verify-local-files", move || {
            candidates
                .into_iter()
                .find(|path| path.is_file() && Format::from_path(path).is_some())
        })
        .await
        .ok()
        .flatten()
}

async fn offer_from_mime_offer(
    offer: &ClipboardMimeOffer,
    tasks: &Scope,
) -> Option<ClipboardOfferKind> {
    let candidates = extract_candidate_local_image_paths(offer);
    if let Some(path) = first_existing_file(candidates, tasks).await {
        return Some(ClipboardOfferKind::LocalImageFile(path));
    }
    first_raw_mime_format(offer).map(ClipboardOfferKind::RawImageMime)
}

async fn origin_from_mime_offer(offer: ClipboardMimeOffer, tasks: &Scope) -> Option<Origin> {
    let candidates = extract_candidate_local_image_paths(&offer);
    if let Some(path) = first_existing_file(candidates, tasks).await {
        return Some(Origin::local(path));
    }
    for mime in MIME_TYPES {
        if let Some(entry) = offer.entries.iter().find(|entry| entry.mime_type == *mime)
            && !entry.bytes.is_empty()
        {
            let hint = Format::from_mime(&entry.mime_type);
            return Some(Origin::Memory {
                kind: MemoryOrigin::Clipboard,
                bytes: Arc::from(entry.bytes.as_slice()),
                format_hint: hint,
            });
        }
    }
    None
}

async fn offer_from_clipboard_item(
    item: &ClipboardItem,
    tasks: &Scope,
) -> Option<ClipboardOfferKind> {
    let candidates = candidate_paths_from_item(item);
    if let Some(path) = first_existing_file(candidates, tasks).await {
        return Some(ClipboardOfferKind::LocalImageFile(path));
    }
    for entry in &item.entries {
        if let ClipboardEntry::Image(image) = entry {
            let fmt = Format::from_mime(image.format.mime_type()).unwrap_or(Format::Png);
            return Some(ClipboardOfferKind::RawImageMime(fmt));
        }
    }
    None
}

async fn origin_from_clipboard_item(item: ClipboardItem, tasks: &Scope) -> Option<Origin> {
    let candidates = candidate_paths_from_item(&item);
    if let Some(path) = first_existing_file(candidates, tasks).await {
        return Some(Origin::local(path));
    }
    for entry in item.entries {
        if let ClipboardEntry::Image(image) = entry {
            let hint = Format::from_mime(image.format.mime_type());
            return Some(Origin::Memory {
                kind: MemoryOrigin::Clipboard,
                bytes: Arc::from(image.bytes),
                format_hint: hint,
            });
        }
    }
    None
}

fn candidate_paths_from_item(item: &ClipboardItem) -> Vec<PathBuf> {
    item.entries
        .iter()
        .filter_map(|entry| match entry {
            ClipboardEntry::ExternalPaths(paths) => Some(paths.paths()),
            _ => None,
        })
        .flatten()
        .filter(|path| Format::from_path(path).is_some())
        .cloned()
        .collect()
}

fn first_raw_mime_format(offer: &ClipboardMimeOffer) -> Option<Format> {
    for mime in MIME_TYPES {
        if let Some(entry) = offer.entries.iter().find(|entry| entry.mime_type == *mime)
            && !entry.bytes.is_empty()
            && let Some(format) = Format::from_mime(&entry.mime_type)
        {
            return Some(format);
        }
    }
    None
}

fn extract_candidate_local_image_paths(offer: &ClipboardMimeOffer) -> Vec<PathBuf> {
    if let Some(gnome) = offer
        .entries
        .iter()
        .find(|entry| entry.mime_type == GNOME_FILES)
    {
        return parse_gnome_files_uris(&gnome.bytes)
            .map(parse_supported_uris)
            .unwrap_or_default();
    }

    if let Some(uri_list) = offer
        .entries
        .iter()
        .find(|entry| entry.mime_type == URI_LIST)
        && uri_list.bytes.len() <= MAX_URI_LIST_BYTES
        && let Ok(text) = std::str::from_utf8(&uri_list.bytes)
    {
        return parse_supported_uris(text);
    }

    Vec::new()
}

fn parse_gnome_files_uris(bytes: &[u8]) -> Option<&str> {
    if bytes.len() > MAX_URI_LIST_BYTES {
        return None;
    }
    let (intent, rest) = std::str::from_utf8(bytes).ok()?.split_once('\n')?;
    let intent = intent.strip_suffix('\r').unwrap_or(intent);
    matches!(intent, "copy" | "cut").then_some(rest)
}

fn parse_supported_uris(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(local_file_uri)
        .filter(|path| Format::from_path(path).is_some())
        .collect()
}

fn local_file_uri(text: &str) -> Option<PathBuf> {
    let absolute = text
        .split_once(':')
        .is_some_and(|(_, rest)| rest.starts_with('/'));
    if text.as_bytes().contains(&0) || !absolute {
        return None;
    }
    let uri = url::Url::parse(text).ok()?;
    if uri.scheme() != "file"
        || uri.host_str().is_some_and(|host| host != "localhost")
        || !uri.username().is_empty()
        || uri.password().is_some()
        || uri.port().is_some()
        || uri.query().is_some()
        || uri.fragment().is_some()
    {
        return None;
    }
    uri.to_file_path().ok()
}
