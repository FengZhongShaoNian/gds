#![allow(dead_code)]
//! Bounded, viewport-prioritized secondary metadata with tab-scoped cancellation.

use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use gds::service::task_manager::Owner as TaskOwner;
use zgui::geom::Point;

use super::ui::Browser;
use crate::entities::file::directory::{self, EnrichmentNeed, Entry};

pub(super) enum Enrichment {
    Running {
        revision: u64,
        need: EnrichmentNeed,
        _work: TaskOwner,
        queue: Arc<Mutex<EnrichQueue>>,
    },
    Complete {
        revision: u64,
        need: EnrichmentNeed,
    },
}

impl Enrichment {
    fn coverage(&self, revision: u64) -> Option<EnrichmentNeed> {
        match self {
            Self::Running {
                revision: current,
                need,
                ..
            }
            | Self::Complete {
                revision: current,
                need,
            } if *current == revision => Some(*need),
            _ => None,
        }
    }
}

struct EnrichBatch {
    entries: Vec<Entry>,
    is_viewport: bool,
}

pub(super) struct EnrichQueue {
    priority: VecDeque<Entry>,
    backlog: VecDeque<Entry>,
    /// Paths already popped by [`Self::pop_batch`] and handed to worker tasks.
    dispatched: HashSet<PathBuf>,
    priority_limit: usize,
    batch_size: usize,
}

impl Default for EnrichQueue {
    fn default() -> Self {
        Self {
            priority: VecDeque::new(),
            backlog: VecDeque::new(),
            dispatched: HashSet::new(),
            priority_limit: directory::DEFAULT_PRIORITY_LIMIT,
            batch_size: directory::DEFAULT_BATCH_SIZE,
        }
    }
}

impl EnrichQueue {
    fn new(
        snapshot: &directory::DirectorySnapshot,
        priority_paths: &[PathBuf],
        need: EnrichmentNeed,
        priority_limit: usize,
        batch_size: usize,
    ) -> Self {
        let batch_size = batch_size.max(1);
        if need.is_empty() {
            return Self {
                priority_limit,
                batch_size,
                ..Self::default()
            };
        }
        let mut priority = VecDeque::with_capacity(priority_paths.len());
        let mut in_priority = HashSet::with_capacity(priority_paths.len());
        for path in priority_paths {
            if let Some(entry) = snapshot.entry_for_path(path)
                && entry.needs_enrichment(need)
                && in_priority.insert(entry.path.clone())
            {
                priority.push_back(entry.clone());
            }
        }

        let mut backlog = VecDeque::new();
        let mut offscreen = 0usize;
        for entry in snapshot.entries.iter() {
            if let directory::ProbeBudget::Bounded(limit) = need.budget
                && offscreen >= limit
            {
                break;
            }
            if !entry.needs_enrichment(need) || in_priority.contains(&entry.path) {
                continue;
            }
            backlog.push_back(entry.clone());
            offscreen += 1;
        }

        Self {
            priority,
            backlog,
            dispatched: HashSet::new(),
            priority_limit,
            batch_size,
        }
    }

    fn is_empty(&self) -> bool {
        self.priority.is_empty() && self.backlog.is_empty()
    }

    fn prioritize_viewport(&mut self, rows: &[Entry], need: EnrichmentNeed) {
        let cap = self.priority_limit.max(rows.len());
        for entry in rows.iter().rev() {
            if !entry.needs_enrichment(need) || self.dispatched.contains(&entry.path) {
                continue;
            }
            if let Some(pos) = self
                .priority
                .iter()
                .position(|existing| existing.path == entry.path)
            {
                self.priority.remove(pos);
            }
            self.priority.push_front(entry.clone());
        }
        while self.priority.len() > cap {
            let Some(evicted) = self.priority.pop_back() else {
                break;
            };
            if need.is_full_budget() {
                self.backlog.push_back(evicted);
            }
        }
    }

    fn pop_batch(&mut self) -> Option<EnrichBatch> {
        let batch_size = self.batch_size.max(1);
        let mut entries = Vec::with_capacity(batch_size);
        while entries.len() < batch_size {
            let Some(entry) = self.priority.pop_front() else {
                break;
            };
            if self.dispatched.insert(entry.path.clone()) {
                entries.push(entry);
            }
        }
        if !entries.is_empty() {
            return Some(EnrichBatch {
                entries,
                is_viewport: true,
            });
        }
        while entries.len() < batch_size {
            let Some(entry) = self.backlog.pop_front() else {
                break;
            };
            if self.dispatched.insert(entry.path.clone()) {
                entries.push(entry);
            }
        }
        (!entries.is_empty()).then_some(EnrichBatch {
            entries,
            is_viewport: false,
        })
    }
}

impl Browser {
    pub(super) fn enrich_directory(&self, _id: u64) {}

    pub(super) fn viewport_changed(&self, _position: Point<zgui::geom::CssPx, zgui::geom::Css>) {}
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf};

    use super::{EnrichQueue, EnrichmentNeed, Entry, directory};

    fn unprobed_dir_snapshot(count: usize) -> directory::DirectorySnapshot {
        let entries: Vec<Entry> = (0..count)
            .map(|index| {
                let name = format!("dir-{index:04}");
                Entry::new(
                    PathBuf::from(format!("/tmp/{name}")),
                    OsString::from(&name),
                    directory::EntryKind::Directory,
                    None,
                    None,
                    directory::EntryDetails::default(),
                )
            })
            .collect();
        directory::DirectorySnapshot {
            path: PathBuf::from("/tmp"),
            entries: entries.into(),
            warnings: Vec::new(),
            options: directory::ReadOptions::default(),
        }
    }

    #[test]
    fn prioritize_viewport_sheds_transient_scroll_items_under_bounded_budget() {
        let snapshot = unprobed_dir_snapshot(300);
        let need = EnrichmentNeed {
            children: true,
            budget: directory::ProbeBudget::Bounded(0),
            ..Default::default()
        };
        let custom_limit = 64usize;
        let mut queue = EnrichQueue::new(
            &snapshot,
            &[],
            need,
            custom_limit,
            directory::DEFAULT_BATCH_SIZE,
        );
        assert!(queue.is_empty());

        // Simulate rapid scrolling across 10 viewports (200 entries total).
        for step in 0..10 {
            let start = step * 20;
            queue.prioritize_viewport(&snapshot.entries[start..start + 20], need);
        }

        let mut popped = Vec::new();
        while let Some(batch) = queue.pop_batch() {
            assert!(batch.is_viewport);
            popped.extend(batch.entries);
        }
        assert_eq!(popped.len(), custom_limit);
        // Most recent viewport (180..200) is popped first; earliest scrolled-past items (0..20) were shed.
        assert_eq!(popped[0].path, snapshot.entries[180].path);
        assert!(
            !popped
                .iter()
                .any(|entry| entry.path == snapshot.entries[0].path)
        );

        // Revisiting an earlier shed viewport later can still enqueue and probe it cleanly.
        queue.prioritize_viewport(&snapshot.entries[0..10], need);
        let revisited = queue.pop_batch().expect("revisited viewport batch");
        assert_eq!(revisited.entries.len(), 10);
        assert_eq!(revisited.entries[0].path, snapshot.entries[0].path);
    }

    #[test]
    fn prioritize_viewport_retains_overflow_in_backlog_under_full_budget() {
        let snapshot = unprobed_dir_snapshot(200);
        let need = EnrichmentNeed {
            children: true,
            budget: directory::ProbeBudget::Full,
            ..Default::default()
        };
        let mut queue = EnrichQueue::new(
            &snapshot,
            &[],
            need,
            directory::DEFAULT_PRIORITY_LIMIT,
            directory::DEFAULT_BATCH_SIZE,
        );

        for step in 0..10 {
            let start = step * 20;
            queue.prioritize_viewport(&snapshot.entries[start..start + 20], need);
        }

        let mut popped = Vec::new();
        while let Some(batch) = queue.pop_batch() {
            popped.extend(batch.entries);
        }
        // Every unique entry is dispatched exactly once under Full budget.
        assert_eq!(popped.len(), 200);
        assert_eq!(popped[0].path, snapshot.entries[180].path);
    }
}
