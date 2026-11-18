//! File clipboard interchange through `zgui`'s text clipboard shim and an in-process cache.
//! Phase 3B uses local `Snapshot` tracking paired with `# gds-files-v1:` text fallback
//! via `try_use_clipboard()` until multi-MIME clipboard support lands in Phase 4.

mod fallback;
mod protocol;
mod state;

use std::{
    cell::RefCell,
    collections::HashSet,
    future::Future,
    path::{Component, Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::SystemTime,
};

use fallback::TextCodec;
use gds::app::Gds;
use protocol::Formats;
use serde::{Deserialize, Serialize};
use state::Cache;
use zgui::prelude::{ClipboardKind, LocalStorage, RwSignal, try_use_clipboard};
use zgui::reactive::{on_cleanup_local, prelude::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Copy,
    Cut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity(pub(super) Option<String>);

impl Identity {
    fn fresh() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        Self(Some(format!(
            "gds:{}:{:?}:{}",
            std::process::id(),
            SystemTime::now(),
            SERIAL.fetch_add(1, Ordering::Relaxed),
        )))
    }

    fn is_owned(&self) -> bool {
        self.0.is_some()
    }

    /// # Errors
    /// Rejects an empty or non-text clipboard identity.
    fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let value = std::str::from_utf8(bytes)
            .map_err(|_| Error::InvalidPayload("clipboard identity must be UTF-8".into()))?;
        if value.is_empty() {
            return Err(Error::InvalidPayload("clipboard identity is empty".into()));
        }
        Ok(Self(Some(value.to_owned())))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub mode: Mode,
    pub paths: Vec<PathBuf>,
    pub identity: Identity,
}

impl Snapshot {
    pub fn can_cancel(&self) -> bool {
        self.mode == Mode::Cut && self.identity.is_owned()
    }

    /// # Errors
    /// Rejects empty selections and paths that are not absolute named filesystem entries.
    fn new(mode: Mode, paths: Vec<PathBuf>) -> Result<Self, Error> {
        let paths = Self::validate_paths(paths)?;
        Ok(Self {
            mode,
            paths,
            identity: Identity::fresh(),
        })
    }

    /// # Errors
    /// Rejects empty or non-local filesystem entry paths.
    fn validate_paths(paths: Vec<PathBuf>) -> Result<Vec<PathBuf>, Error> {
        let mut seen = HashSet::new();
        let mut unique = Vec::with_capacity(paths.len());
        for path in paths {
            if !path.is_absolute()
                || !matches!(path.components().next_back(), Some(Component::Normal(_)))
                || path.as_os_str().as_encoded_bytes().contains(&0)
            {
                return Err(Error::InvalidPath(path));
            }
            if seen.insert(path.clone()) {
                unique.push(path);
            }
        }
        if unique.is_empty() {
            return Err(Error::EmptySelection);
        }
        Ok(unique)
    }

    fn excluding(&self, current: &Self, removed: &[PathBuf]) -> Option<Vec<PathBuf>> {
        if self != current {
            return None;
        }
        let removed: HashSet<&Path> = removed.iter().map(PathBuf::as_path).collect();
        let remaining: Vec<_> = self
            .paths
            .iter()
            .filter(|path| !removed.contains(path.as_path()))
            .cloned()
            .collect();
        (remaining.len() != self.paths.len()).then_some(remaining)
    }

    fn remaining_after(&self, current: &Self, succeeded: &[PathBuf]) -> Option<Vec<PathBuf>> {
        // Foreign offers have no per-copy identity. Do not overwrite their owner,
        // even if another copy happens to contain exactly the same path list.
        if self.mode != Mode::Cut || !self.identity.is_owned() || self != current {
            return None;
        }
        let completed: HashSet<&Path> = succeeded.iter().map(PathBuf::as_path).collect();
        let remaining: Vec<_> = self
            .paths
            .iter()
            .filter(|path| !completed.contains(path.as_path()))
            .cloned()
            .collect();
        (remaining.len() != self.paths.len()).then_some(remaining)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no files are selected")]
    EmptySelection,
    #[error("clipboard path is not an absolute named filesystem entry: {0}")]
    InvalidPath(PathBuf),
    #[error("clipboard contains an unsupported or non-local file URI: {0}")]
    InvalidUri(String),
    #[error("invalid file clipboard data: {0}")]
    InvalidPayload(String),
    #[error("file clipboard payload exceeds the supported size")]
    TooLarge,
    #[error("file clipboard I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("file clipboard data could not be encoded or decoded: {0}")]
    Json(#[from] serde_json::Error),
}

thread_local! {
    static INSTALLED: RefCell<Option<Clipboard>> = const { RefCell::new(None) };
}

#[allow(dead_code)]
pub(crate) enum Payload {
    Empty,
    Local(Snapshot),
    Uris { mode: Mode, uris: Vec<String> },
}

#[derive(Clone)]
pub(crate) struct Clipboard(Rc<ClipboardState>);

struct ClipboardState {
    cache: RefCell<Cache>,
    revision: RwSignal<u64, LocalStorage>,
}

impl Clipboard {
    pub fn current() -> Self {
        if let Some(clipboard) = INSTALLED.with(|slot| slot.borrow().clone()) {
            return clipboard;
        }
        let install = || {
            let clipboard = Self(Rc::new(ClipboardState {
                cache: RefCell::new(Cache::default()),
                revision: RwSignal::new_local(0),
            }));
            INSTALLED.with(|slot| *slot.borrow_mut() = Some(clipboard.clone()));
            on_cleanup_local(|| {
                INSTALLED.with(|slot| slot.borrow_mut().take());
            });
            clipboard
        };
        match Gds::try_current() {
            Some(gds) => gds.with(install),
            None => install(),
        }
    }

    fn changed(&self) {
        self.0
            .revision
            .update(|revision| *revision = revision.wrapping_add(1));
    }

    pub fn snapshot(&self) -> Option<Arc<Snapshot>> {
        self.0.revision.get();
        self.0.cache.borrow().current()
    }

    pub fn is_cut(&self, path: &Path) -> bool {
        self.0.revision.get();
        self.0.cache.borrow().is_cut(path)
    }

    pub fn can_paste(&self) -> bool {
        self.0.revision.get();
        self.0.cache.borrow().can_paste()
    }

    pub fn refresh(&self) {
        let Some(clip) = try_use_clipboard() else {
            return;
        };
        let Some(generation) = self.0.cache.borrow_mut().begin_refresh() else {
            return;
        };
        let weak = Rc::downgrade(&self.0);
        let cancel = Gds::try_current().map(|gds| gds.tasks().scope().token());
        clip.read_text(ClipboardKind::Standard, move |text| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if cancel.as_ref().is_some_and(|token| token.is_cancelled()) {
                return;
            }
            let clipboard = Clipboard(state);
            let parsed = TextCodec::decode_uris(text.as_deref());
            let (result, remote) = match (TextCodec::decode_snapshot(text.as_deref()), parsed) {
                (Ok(snapshot), Ok(Some((mode, uris)))) => {
                    let remote_copy =
                        snapshot.is_none() && mode == Mode::Copy && Formats::copyable_uris(&uris);
                    (Ok(snapshot), remote_copy)
                }
                (Ok(snapshot), Ok(None)) => (Ok(snapshot), false),
                (Err(Error::InvalidUri(_)), Ok(Some((mode, uris)))) => {
                    let remote_copy = mode == Mode::Copy && Formats::copyable_uris(&uris);
                    (Ok(None), remote_copy)
                }
                (Err(error), _) | (_, Err(error)) => (Err(error), false),
            };
            let changed = clipboard
                .0
                .cache
                .borrow_mut()
                .reconcile(generation, result, remote);
            if changed {
                clipboard.changed();
            }
        });
    }

    /// # Errors
    /// Reports invalid selections or clipboard serialization failures.
    #[allow(dead_code)]
    pub async fn write(&self, mode: Mode, paths: Vec<PathBuf>) -> Result<Snapshot, Error> {
        let snapshot = Snapshot::new(mode, paths)?;
        let encoded = TextCodec::encode_snapshot(&snapshot)?;
        if let Some(clip) = try_use_clipboard() {
            clip.set_text(ClipboardKind::Standard, encoded);
        }
        self.0.cache.borrow_mut().record_write(&snapshot);
        self.changed();
        Ok(snapshot)
    }

    /// # Errors
    /// Reports invalid file clipboard payloads.
    #[allow(dead_code)]
    pub async fn read(&self) -> Result<Option<Snapshot>, Error> {
        if let Some(text) = read_system_clipboard_text().await {
            return TextCodec::decode_snapshot(Some(&text));
        }
        Ok(self.0.cache.borrow().current().map(|s| (*s).clone()))
    }

    /// # Errors
    /// Reports invalid file or remote URI payloads.
    #[allow(dead_code)]
    pub async fn read_payload(&self) -> Result<Payload, Error> {
        if let Some(text) = read_system_clipboard_text().await {
            return match TextCodec::decode_snapshot(Some(&text)) {
                Ok(Some(snapshot)) => Ok(Payload::Local(snapshot)),
                Ok(None) => match TextCodec::decode_uris(Some(&text))? {
                    Some((mode, uris)) => Ok(Payload::Uris { mode, uris }),
                    None => Ok(Payload::Empty),
                },
                Err(Error::InvalidUri(_)) => match TextCodec::decode_uris(Some(&text))? {
                    Some((mode, uris)) => Ok(Payload::Uris { mode, uris }),
                    None => Ok(Payload::Empty),
                },
                Err(error) => Err(error),
            };
        }
        Ok(match self.0.cache.borrow().current() {
            Some(snapshot) => Payload::Local((*snapshot).clone()),
            None => Payload::Empty,
        })
    }

    /// # Errors
    /// Reports invalid remaining paths or serialization failures.
    #[allow(dead_code)]
    pub async fn remove(&self, expected: Arc<Snapshot>, paths: Vec<PathBuf>) -> Result<(), Error> {
        let result = self.remove_paths(expected, paths, true).await;
        self.refresh();
        result
    }

    /// # Errors
    /// Reports malformed URI lists or serialization failures.
    #[allow(dead_code)]
    pub async fn read_uris(&self) -> Result<Option<(Mode, Vec<String>)>, Error> {
        if let Some(text) = read_system_clipboard_text().await {
            return TextCodec::decode_uris(Some(&text));
        }
        let cached = self.0.cache.borrow().current();
        let Some(snapshot) = cached else {
            return Ok(None);
        };
        let uris = snapshot
            .paths
            .iter()
            .map(|path| {
                url::Url::from_file_path(path)
                    .map(|uri| uri.to_string())
                    .map_err(|_| Error::InvalidPath(path.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some((snapshot.mode, uris)))
    }

    /// # Errors
    /// Reports invalid or uncopyable remote URIs.
    #[allow(dead_code)]
    pub async fn write_uris(&self, uris: Vec<String>) -> Result<(), Error> {
        let checked = Formats::parse_uri_list(&uris.join("\n"))?;
        let copyable = Formats::copyable_uris(&checked);
        let encoded = TextCodec::encode_uris(&checked)?;
        if let Some(clip) = try_use_clipboard() {
            clip.set_text(ClipboardKind::Standard, encoded);
        }
        self.0.cache.borrow_mut().record_uri_copy(copyable);
        self.changed();
        Ok(())
    }

    /// # Errors
    /// Reports clipboard update failures while leaving completed filesystem moves intact.
    #[allow(dead_code)]
    pub async fn finish_cut(
        &self,
        expected: Snapshot,
        succeeded: Vec<PathBuf>,
    ) -> Result<(), Error> {
        let snapshot = expected.clone();
        let removed = succeeded.clone();
        let result = self
            .remove_paths(Arc::new(expected), succeeded, false)
            .await;
        if result.is_ok() {
            self.0.cache.borrow_mut().note_pasted(&snapshot, &removed);
            self.changed();
            self.refresh();
        }
        result
    }

    #[allow(dead_code)]
    pub fn cancel_cut(&self) -> Option<impl Future<Output = Result<(), Error>> + 'static> {
        let expected = self.0.cache.borrow_mut().begin_cancel()?;
        let clipboard = self.clone();
        Some(async move {
            let result = clipboard
                .remove_paths(expected.clone(), expected.paths.clone(), false)
                .await;
            clipboard
                .0
                .cache
                .borrow_mut()
                .finish_cancel(&expected, result.is_ok());
            clipboard.changed();
            clipboard.refresh();
            result
        })
    }

    async fn remove_paths(
        &self,
        expected: Arc<Snapshot>,
        removed: Vec<PathBuf>,
        explicit: bool,
    ) -> Result<(), Error> {
        let current = if let Some(text) = read_system_clipboard_text().await {
            TextCodec::decode_snapshot(Some(&text))?
        } else {
            self.0.cache.borrow().current().map(|s| (*s).clone())
        };
        let Some(current) = current else {
            return Ok(());
        };
        let remaining = if explicit {
            expected.excluding(&current, &removed)
        } else {
            expected.remaining_after(&current, &removed)
        };
        let Some(remaining) = remaining else {
            return Ok(());
        };
        if remaining.is_empty() {
            if let Some(clip) = try_use_clipboard() {
                clip.clear(ClipboardKind::Standard);
            }
            self.0.cache.borrow_mut().clear();
        } else {
            let next = Snapshot::new(expected.mode, remaining)?;
            let encoded = TextCodec::encode_snapshot(&next)?;
            if let Some(clip) = try_use_clipboard() {
                clip.set_text(ClipboardKind::Standard, encoded);
            }
            self.0.cache.borrow_mut().record_write(&next);
        }
        self.changed();
        Ok(())
    }
}

async fn read_system_clipboard_text() -> Option<String> {
    let clip = try_use_clipboard()?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    clip.read_text(ClipboardKind::Standard, move |text| {
        let _ = sender.send(text);
    });
    receiver.await.ok().flatten()
}

#[cfg(test)]
mod tests;
