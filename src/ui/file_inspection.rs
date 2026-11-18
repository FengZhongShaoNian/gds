//! Shared asynchronous inspection cache for file previews and details.

#[cfg(test)]
mod tests;

use std::sync::Arc;

use zgui::reactive::{LocalStorage, RwSignal, prelude::*};
use zgui_image::ImageBytes;

use crate::service::file_inspection::{Metadata, Request, Source};

pub const DEFAULT_CACHE_ENTRIES: usize = 256;
pub const DEFAULT_CACHE_MIB: usize = 128;
pub const DEFAULT_WORKERS: usize = 2;

/// Configurable cache bounds and worker options for [`Cache`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Options {
    pub capacity: usize,
    pub byte_budget: usize,
    pub workers: usize,
    pub inspection: crate::service::file_inspection::Options,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CACHE_ENTRIES,
            byte_budget: DEFAULT_CACHE_MIB * 1024 * 1024,
            workers: DEFAULT_WORKERS,
            inspection: crate::service::file_inspection::Options::default(),
        }
    }
}

/// Asynchronous lookup state for file metadata or thumbnail previews.
#[derive(Clone, Default)]
pub enum State<T> {
    #[default]
    NotRequested,
    Loading,
    Ready(Arc<T>),
    Unavailable,
    TooLarge,
    Failed(Arc<str>),
}

/// Cached inspection snapshot for one file source.
#[derive(Clone, Default)]
pub struct Snapshot {
    pub metadata: State<Metadata>,
    pub thumbnail: State<ImageBytes>,
}

/// Reactive inspection and thumbnail cache bound to a workspace task scope.
#[derive(Clone, Default)]
pub struct Cache {
    options: Options,
    revision: Option<RwSignal<u64, LocalStorage>>,
}

impl Cache {
    #[must_use]
    pub fn new(options: Options) -> Self {
        Self {
            options,
            revision: Some(RwSignal::new_local(0)),
        }
    }

    #[must_use]
    pub fn options(&self) -> Options {
        self.options
    }

    #[must_use]
    pub fn snapshot(&self, source: &Source, request: Request) -> Snapshot {
        let _ = (source, request);
        if let Some(rev) = self.revision {
            let _ = rev.get();
        }
        Snapshot::default()
    }
}
