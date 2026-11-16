//! Reversible spatial and annotation editing commands.

use std::sync::Arc;

use gds::service::image::PixelRect;

use crate::entities::document::{AnnotationId, AnnotationItem, DocumentState, SpatialTransformOp};

/// Reversible image transform or annotation mutation stored on [`super::EditHistory`].
#[derive(Clone, Debug, PartialEq)]
pub enum EditCommand {
    RotateCw {
        before_annotations: Arc<[AnnotationItem]>,
        after_annotations: Arc<[AnnotationItem]>,
    },
    RotateCcw {
        before_annotations: Arc<[AnnotationItem]>,
        after_annotations: Arc<[AnnotationItem]>,
    },
    FlipHorizontal {
        before_annotations: Arc<[AnnotationItem]>,
        after_annotations: Arc<[AnnotationItem]>,
    },
    FlipVertical {
        before_annotations: Arc<[AnnotationItem]>,
        after_annotations: Arc<[AnnotationItem]>,
    },
    SetCrop {
        previous: Option<PixelRect>,
        next: Option<PixelRect>,
        before_annotations: Arc<[AnnotationItem]>,
        after_annotations: Arc<[AnnotationItem]>,
    },
    AddAnnotations(Arc<[AnnotationItem]>),
    UpdateAnnotations {
        before: Arc<[AnnotationItem]>,
        after: Arc<[AnnotationItem]>,
    },
    RemoveAnnotations {
        /// Stored in ascending original index order so undo re-inserts low-to-high cleanly.
        removed: Arc<[(usize, AnnotationItem)]>,
    },
    ReorderAnnotations {
        before_order: Arc<[AnnotationId]>,
        after_order: Arc<[AnnotationId]>,
    },
}

impl EditCommand {
    pub fn build_spatial_transform(op: SpatialTransformOp, doc: &DocumentState) -> Self {
        let (out_w, out_h) = doc.output_dimensions();
        let before_annotations: Arc<[AnnotationItem]> = Arc::from(doc.annotations.as_slice());
        let after_vec: Vec<AnnotationItem> = doc
            .annotations
            .iter()
            .map(|item| item.apply_spatial_transform(op, out_w, out_h))
            .collect();
        let after_annotations: Arc<[AnnotationItem]> = Arc::from(after_vec);
        match op {
            SpatialTransformOp::RotateCw => Self::RotateCw {
                before_annotations,
                after_annotations,
            },
            SpatialTransformOp::RotateCcw => Self::RotateCcw {
                before_annotations,
                after_annotations,
            },
            SpatialTransformOp::FlipHorizontal => Self::FlipHorizontal {
                before_annotations,
                after_annotations,
            },
            SpatialTransformOp::FlipVertical => Self::FlipVertical {
                before_annotations,
                after_annotations,
            },
        }
    }

    pub fn build_crop(base_crop: PixelRect, output_crop: PixelRect, doc: &DocumentState) -> Self {
        let before_annotations: Arc<[AnnotationItem]> = Arc::from(doc.annotations.as_slice());
        let after_vec: Vec<AnnotationItem> = doc
            .annotations
            .iter()
            .filter_map(|item| item.apply_crop(output_crop))
            .collect();
        let after_annotations: Arc<[AnnotationItem]> = Arc::from(after_vec);
        Self::SetCrop {
            previous: doc.transform.spec.crop,
            next: Some(base_crop),
            before_annotations,
            after_annotations,
        }
    }

    pub fn apply(&self, doc: &mut DocumentState) {
        match self {
            Self::RotateCw {
                after_annotations, ..
            } => {
                doc.transform.spec.rotation = doc.transform.spec.rotation.rotate_cw();
                doc.annotations = after_annotations.to_vec();
            }
            Self::RotateCcw {
                after_annotations, ..
            } => {
                doc.transform.spec.rotation = doc.transform.spec.rotation.rotate_ccw();
                doc.annotations = after_annotations.to_vec();
            }
            Self::FlipHorizontal {
                after_annotations, ..
            } => {
                doc.transform.spec.flip_h = !doc.transform.spec.flip_h;
                doc.annotations = after_annotations.to_vec();
            }
            Self::FlipVertical {
                after_annotations, ..
            } => {
                doc.transform.spec.flip_v = !doc.transform.spec.flip_v;
                doc.annotations = after_annotations.to_vec();
            }
            Self::SetCrop {
                next,
                after_annotations,
                ..
            } => {
                doc.transform.spec.crop = *next;
                doc.annotations = after_annotations.to_vec();
            }
            Self::AddAnnotations(items) => {
                doc.annotations.extend(items.iter().cloned());
            }
            Self::UpdateAnnotations { after, .. } => {
                apply_item_updates(&mut doc.annotations, after);
            }
            Self::RemoveAnnotations { removed } => {
                doc.annotations
                    .retain(|item| !removed.iter().any(|(_, entry)| entry.id == item.id));
            }
            Self::ReorderAnnotations { after_order, .. } => {
                reorder_by_ids(&mut doc.annotations, after_order);
            }
        }
        doc.bump_revision();
    }

    pub fn undo(&self, doc: &mut DocumentState) {
        match self {
            Self::RotateCw {
                before_annotations, ..
            } => {
                doc.transform.spec.rotation = doc.transform.spec.rotation.rotate_ccw();
                doc.annotations = before_annotations.to_vec();
            }
            Self::RotateCcw {
                before_annotations, ..
            } => {
                doc.transform.spec.rotation = doc.transform.spec.rotation.rotate_cw();
                doc.annotations = before_annotations.to_vec();
            }
            Self::FlipHorizontal {
                before_annotations, ..
            } => {
                doc.transform.spec.flip_h = !doc.transform.spec.flip_h;
                doc.annotations = before_annotations.to_vec();
            }
            Self::FlipVertical {
                before_annotations, ..
            } => {
                doc.transform.spec.flip_v = !doc.transform.spec.flip_v;
                doc.annotations = before_annotations.to_vec();
            }
            Self::SetCrop {
                previous,
                before_annotations,
                ..
            } => {
                doc.transform.spec.crop = *previous;
                doc.annotations = before_annotations.to_vec();
            }
            Self::AddAnnotations(items) => {
                doc.annotations
                    .retain(|entry| !items.iter().any(|added| added.id == entry.id));
            }
            Self::UpdateAnnotations { before, .. } => {
                apply_item_updates(&mut doc.annotations, before);
            }
            Self::RemoveAnnotations { removed } => {
                for (idx, item) in removed.iter() {
                    let pos = (*idx).min(doc.annotations.len());
                    doc.annotations.insert(pos, item.clone());
                }
            }
            Self::ReorderAnnotations { before_order, .. } => {
                reorder_by_ids(&mut doc.annotations, before_order);
            }
        }
        doc.bump_revision();
    }
}

fn apply_item_updates(annotations: &mut [AnnotationItem], updates: &[AnnotationItem]) {
    for replacement in updates {
        if let Some(slot) = annotations
            .iter_mut()
            .find(|entry| entry.id == replacement.id)
        {
            *slot = replacement.clone();
        }
    }
}

fn reorder_by_ids(annotations: &mut Vec<AnnotationItem>, target_order: &[AnnotationId]) {
    let mut existing = std::mem::take(annotations);
    annotations.reserve(existing.len());
    for &id in target_order {
        if let Some(pos) = existing.iter().position(|item| item.id == id) {
            annotations.push(existing.remove(pos));
        }
    }
    annotations.extend(existing);
}
