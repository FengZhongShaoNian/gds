//! Reversible spatial and annotation editing commands.

use std::sync::Arc;

use gds::service::image::PixelRect;

use crate::entities::document::{AnnotationItem, DocumentState};

/// Reversible image transform or annotation mutation stored on [`super::EditHistory`].
#[derive(Clone, Debug, PartialEq)]
pub enum EditCommand {
    RotateCw {
        prev_annotations: Arc<[AnnotationItem]>,
    },
    RotateCcw {
        prev_annotations: Arc<[AnnotationItem]>,
    },
    FlipHorizontal {
        prev_annotations: Arc<[AnnotationItem]>,
    },
    FlipVertical {
        prev_annotations: Arc<[AnnotationItem]>,
    },
    SetCrop {
        previous: Option<PixelRect>,
        next: Option<PixelRect>,
        prev_annotations: Arc<[AnnotationItem]>,
    },
    AddAnnotation(AnnotationItem),
}

impl EditCommand {
    pub fn apply(&self, doc: &mut DocumentState) {
        match self {
            Self::RotateCw { .. } => {
                doc.transform.spec.rotation = doc.transform.spec.rotation.rotate_cw();
                doc.annotations.clear();
            }
            Self::RotateCcw { .. } => {
                doc.transform.spec.rotation = doc.transform.spec.rotation.rotate_ccw();
                doc.annotations.clear();
            }
            Self::FlipHorizontal { .. } => {
                doc.transform.spec.flip_h = !doc.transform.spec.flip_h;
                doc.annotations.clear();
            }
            Self::FlipVertical { .. } => {
                doc.transform.spec.flip_v = !doc.transform.spec.flip_v;
                doc.annotations.clear();
            }
            Self::SetCrop { next, .. } => {
                doc.transform.spec.crop = *next;
                doc.annotations.clear();
            }
            Self::AddAnnotation(item) => {
                doc.annotations.push(item.clone());
            }
        }
        doc.bump_revision();
    }

    pub fn undo(&self, doc: &mut DocumentState) {
        match self {
            Self::RotateCw { prev_annotations } => {
                doc.transform.spec.rotation = doc.transform.spec.rotation.rotate_ccw();
                doc.annotations = prev_annotations.to_vec();
            }
            Self::RotateCcw { prev_annotations } => {
                doc.transform.spec.rotation = doc.transform.spec.rotation.rotate_cw();
                doc.annotations = prev_annotations.to_vec();
            }
            Self::FlipHorizontal { prev_annotations } => {
                doc.transform.spec.flip_h = !doc.transform.spec.flip_h;
                doc.annotations = prev_annotations.to_vec();
            }
            Self::FlipVertical { prev_annotations } => {
                doc.transform.spec.flip_v = !doc.transform.spec.flip_v;
                doc.annotations = prev_annotations.to_vec();
            }
            Self::SetCrop {
                previous,
                prev_annotations,
                ..
            } => {
                doc.transform.spec.crop = *previous;
                doc.annotations = prev_annotations.to_vec();
            }
            Self::AddAnnotation(item) => {
                if let Some(pos) = doc
                    .annotations
                    .iter()
                    .rposition(|entry| entry.id == item.id)
                {
                    doc.annotations.remove(pos);
                }
            }
        }
        doc.bump_revision();
    }
}
