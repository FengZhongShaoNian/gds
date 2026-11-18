#![allow(dead_code)]
//! Native tab reordering and lazy file drag snapshots.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use zgui::prelude::*;
use zgui::view::EventCx;

use super::{geometry::Grid, model::Row, ui::Browser};
use crate::{
    entities::file::{
        directory::{Entry, EntryKind},
        ui::FileIcon,
    },
    shared::lib::selection::Selection,
};

#[derive(Clone, Debug)]
struct DragItem {
    path: PathBuf,
    name: String,
    is_dir: bool,
    icon: FileIcon,
    directory_icon: bool,
}

impl DragItem {
    fn new(entry: &Entry) -> Self {
        Self {
            path: entry.path.clone(),
            name: entry.display_name.clone(),
            is_dir: entry.kind == EntryKind::Directory,
            icon: FileIcon::for_entry(entry),
            directory_icon: FileIcon::is_directory(entry),
        }
    }
}

#[derive(Debug)]
struct DragSnapshotSource {
    rows: Arc<[Entry]>,
    selected: Selection<PathBuf>,
}

#[derive(Debug)]
pub(in crate::pages::workspace) struct DragSnapshotInner {
    source: std::sync::Mutex<Option<DragSnapshotSource>>,
    entries: OnceLock<Arc<[DragItem]>>,
}

impl DragSnapshotInner {
    fn entries(&self) -> &[DragItem] {
        self.entries
            .get_or_init(|| {
                let Some(source) = self
                    .source
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                else {
                    return Arc::from([]);
                };
                source
                    .rows
                    .iter()
                    .filter(|row| source.selected.contains(&row.path))
                    .map(DragItem::new)
                    .collect::<Vec<_>>()
                    .into()
            })
            .as_ref()
    }

    fn clear_source(&self) {
        self.source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    fn set_rows(&self, rows: Arc<[Entry]>) {
        if self.entries.get().is_some() {
            return;
        }
        if let Some(source) = self
            .source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_mut()
        {
            source.rows = rows;
        }
    }
}

#[derive(Clone, Debug)]
enum DragPayload {
    Single(Entry),
    Selection(Arc<DragSnapshotInner>),
}

/// Files carried by a drag that started in the file table.
#[derive(Clone, Debug)]
pub(super) struct FileDrag {
    payload: DragPayload,
}

impl FileDrag {
    fn selection(inner: Arc<DragSnapshotInner>) -> Self {
        Self {
            payload: DragPayload::Selection(inner),
        }
    }

    fn single(entry: &Entry) -> Self {
        Self {
            payload: DragPayload::Single(entry.clone()),
        }
    }

    pub(super) fn is_source(_path: &Path) -> bool {
        false
    }

    pub(super) fn paths(&self) -> Vec<PathBuf> {
        match &self.payload {
            DragPayload::Single(entry) => vec![entry.path.clone()],
            DragPayload::Selection(inner) => inner
                .entries()
                .iter()
                .map(|item| item.path.clone())
                .collect(),
        }
    }

    pub(super) fn count(&self) -> usize {
        match &self.payload {
            DragPayload::Single(_) => 1,
            DragPayload::Selection(inner) => inner.entries().len(),
        }
    }
}

/// Frozen selection for drag payloads whose item slice materializes lazily on drag start.
#[derive(Clone, Default)]
pub(super) struct DragSnapshot {
    inner: Option<Arc<DragSnapshotInner>>,
}

pub(super) struct SuspendedDrag {
    inner: Option<Arc<DragSnapshotInner>>,
    selected: Selection<PathBuf>,
}

impl DragSnapshot {
    pub(super) fn new(rows: &Arc<[Entry]>, selected: &Selection<PathBuf>) -> Self {
        if rows.is_empty() || selected.len() <= 1 {
            return Self { inner: None };
        }
        Self {
            inner: Some(Arc::new(DragSnapshotInner {
                source: std::sync::Mutex::new(Some(DragSnapshotSource {
                    rows: Arc::clone(rows),
                    selected: selected.clone(),
                })),
                entries: OnceLock::new(),
            })),
        }
    }

    pub(super) fn clear_source(&mut self) {
        if let Some(inner) = self.inner.take() {
            inner.clear_source();
        }
    }

    pub(super) fn suspend_rows(&mut self) -> Option<SuspendedDrag> {
        let inner = self.inner.take()?;
        let source = inner
            .source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()?;
        Some(SuspendedDrag {
            inner: Some(inner),
            selected: source.selected,
        })
    }

    pub(super) fn restore_rows(
        &mut self,
        suspended: Option<SuspendedDrag>,
        rows: &Arc<[Entry]>,
        selected: &Selection<PathBuf>,
    ) {
        let Some(mut suspended) = suspended else {
            *self = Self::new(rows, selected);
            return;
        };
        if rows.is_empty() || suspended.selected.len() <= 1 {
            self.inner = None;
            return;
        }
        if let Some(inner) = suspended.inner.take() {
            if inner.entries.get().is_none() {
                *inner
                    .source
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some(DragSnapshotSource {
                        rows: Arc::clone(rows),
                        selected: suspended.selected,
                    });
            }
            self.inner = Some(inner);
        } else {
            *self = Self::new(rows, selected);
        }
    }

    pub(super) fn update_rows(&mut self, rows: &Arc<[Entry]>, selected: &Selection<PathBuf>) {
        if let Some(inner) = &self.inner {
            inner.set_rows(Arc::clone(rows));
        } else {
            *self = Self::new(rows, selected);
        }
    }

    #[cfg(test)]
    pub(super) fn drag_for(&self, entry: &Entry, selected: &Selection<PathBuf>) -> FileDrag {
        self.drag_for_row(entry, selected.contains(&entry.path))
    }

    pub(super) fn drag_for_row(&self, entry: &Entry, selected: bool) -> FileDrag {
        match (&self.inner, selected) {
            (Some(inner), true) => FileDrag::selection(Arc::clone(inner)),
            _ => FileDrag::single(entry),
        }
    }

    #[cfg(test)]
    pub(super) fn is_materialized(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|inner| inner.entries.get().is_some())
    }
}

impl Browser {
    pub(super) fn background_drop_target(&self, view: AnyView) -> AnyView {
        view
    }

    pub(super) fn draggable_tab(&self, view: AnyView, _id: u64, _pinned: bool) -> AnyView {
        view
    }

    pub(super) fn file_drag_target(&self, view: AnyView, _destination: PathBuf) -> AnyView {
        view
    }

    pub(super) fn file_row_drag(&self, view: AnyView, _row: &Row, _grid: Option<Grid>) -> AnyView {
        view
    }

    pub(super) fn mouse_navigation(
        &self,
        _event: &mut EventCx<'_, zgui::view::events::PointerDown>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use super::{DragSnapshot, Entry, EntryKind, FileDrag};
    use crate::{entities::file::directory::EntryDetails, shared::lib::selection::Selection};

    fn sample_entry(index: usize) -> Entry {
        let name = format!("file-{index:04}.txt");
        Entry::new(
            PathBuf::from(format!("/tmp/drag-test/{name}")),
            name.into(),
            EntryKind::File,
            Some(128),
            None,
            EntryDetails::default(),
        )
    }

    #[test]
    fn drag_for_row_defers_selection_materialization_until_drag_starts() {
        let rows: Arc<[Entry]> = (0..5_000).map(sample_entry).collect::<Vec<_>>().into();
        let selected: Selection<PathBuf> = rows[..1_000]
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        let snapshot = DragSnapshot::new(&rows, &selected);

        let selected_drags: Vec<FileDrag> = rows[..30]
            .iter()
            .map(|entry| snapshot.drag_for_row(entry, true))
            .collect();
        let unselected_drags: Vec<FileDrag> = rows[1_000..1_030]
            .iter()
            .map(|entry| snapshot.drag_for(entry, &selected))
            .collect();
        assert!(!snapshot.is_materialized());

        assert_eq!(unselected_drags[0].paths(), vec![rows[1_000].path.clone()]);
        assert_eq!(unselected_drags[0].count(), 1);
        assert!(!snapshot.is_materialized());

        let expected_selected: Vec<PathBuf> = rows[..1_000]
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        assert_eq!(selected_drags[0].paths(), expected_selected);
        assert!(snapshot.is_materialized());
        assert_eq!(selected_drags[15].count(), 1_000);
    }
}
