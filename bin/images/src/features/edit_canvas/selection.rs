//! Ordered annotation selection set with deterministic hashing and document reconciliation.

use crate::{
    entities::document::{AnnotationId, AnnotationItem},
    shared::geometry::Rect2,
};

/// Ordered set of selected annotation IDs; the last element is [`Self::primary`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectionSet {
    ids: Vec<AnnotationId>,
}

impl SelectionSet {
    pub const fn empty() -> Self {
        Self { ids: Vec::new() }
    }

    pub fn single(id: AnnotationId) -> Self {
        Self { ids: vec![id] }
    }

    pub fn from_ids(ids: impl IntoIterator<Item = AnnotationId>) -> Self {
        let mut set = Self::empty();
        for id in ids {
            set.add(id);
        }
        set
    }

    pub const fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub const fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn contains(&self, id: AnnotationId) -> bool {
        self.ids.contains(&id)
    }

    pub fn primary(&self) -> Option<AnnotationId> {
        self.ids.last().copied()
    }

    pub fn clear(&mut self) -> bool {
        if self.ids.is_empty() {
            return false;
        }
        self.ids.clear();
        true
    }

    pub fn set_single(&mut self, id: AnnotationId) -> bool {
        if self.ids.len() == 1 && self.ids[0] == id {
            return false;
        }
        self.ids.clear();
        self.ids.push(id);
        true
    }

    pub fn add(&mut self, id: AnnotationId) -> bool {
        if self.contains(id) {
            return false;
        }
        self.ids.push(id);
        true
    }

    pub fn toggle(&mut self, id: AnnotationId) {
        if let Some(pos) = self.ids.iter().position(|&existing| existing == id) {
            self.ids.remove(pos);
        } else {
            self.ids.push(id);
        }
    }

    pub fn select_all(&mut self, annotations: &[AnnotationItem]) -> bool {
        let next: Vec<AnnotationId> = annotations.iter().map(|item| item.id).collect();
        if self.ids == next {
            return false;
        }
        self.ids = next;
        true
    }

    /// Removes IDs that no longer exist in `annotations`, returning whether selection changed.
    pub fn reconcile(&mut self, annotations: &[AnnotationItem]) -> bool {
        let before = self.ids.len();
        self.ids
            .retain(|id| annotations.iter().any(|item| item.id == *id));
        self.ids.len() != before
    }

    /// Deterministic FNV-1a hash of the active selection set for history coalescing.
    pub fn selection_hash(&self) -> u64 {
        if self.ids.is_empty() {
            return 0;
        }
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for id in &self.ids {
            for byte in id.0.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
        hash
    }

    pub fn indices_ascending(&self, annotations: &[AnnotationItem]) -> Vec<usize> {
        annotations
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| self.contains(item.id).then_some(idx))
            .collect()
    }

    pub fn selected_items_ordered(&self, annotations: &[AnnotationItem]) -> Vec<AnnotationItem> {
        annotations
            .iter()
            .filter(|item| self.contains(item.id))
            .cloned()
            .collect()
    }

    pub fn union_bounds(&self, annotations: &[AnnotationItem]) -> Option<Rect2> {
        let mut union: Option<Rect2> = None;
        for item in annotations {
            if !self.contains(item.id) {
                continue;
            }
            let bounds = item.visual_bounds();
            union = Some(union.map_or(bounds, |acc| acc.union(bounds)));
        }
        union
    }
}
