use std::sync::Arc;

use gds::service::image::{
    PixelRect,
    decode::DecodedImage,
    metadata::{EditableFields, ExifSummary, MetadataBundle, TechnicalFacts},
    ops::OrthogonalRotation,
    source::{Format, MemoryOrigin, Origin},
};
use image::{Rgba, RgbaImage};

use super::{EditCommand, EditHistory, MAX_UNDO_DEPTH, ToolFsm, ToolMode, tool_fsm::FinishOutcome};
use crate::{
    entities::document::{AnnotationKind, DocumentState},
    shared::geometry::Point2,
};

fn dummy_document(width: u32, height: u32) -> DocumentState {
    let mut rgba = RgbaImage::new(width, height);
    rgba.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    let decoded = DecodedImage {
        origin: Origin::Memory {
            kind: MemoryOrigin::Clipboard,
            bytes: Arc::from([]),
            format_hint: Some(Format::Png),
        },
        format: Format::Png,
        rgba: Arc::new(rgba),
        metadata: MetadataBundle {
            technical: TechnicalFacts {
                format: Format::Png,
                width,
                height,
                file_bytes: Some(0),
                color_type: "Rgba8".to_owned(),
                has_alpha: true,
                orientation: 1,
                icc_profile_name: None,
                has_icc_profile: false,
                has_embedded_thumbnail: false,
                frame_count: None,
            },
            exif: ExifSummary::default(),
            editable: EditableFields::default(),
            raw_exif: None,
            raw_icc: None,
        },
        was_color_managed: false,
        is_preview_downscaled: false,
        source_bytes: Arc::from([]),
    };
    DocumentState::from_decoded(decoded)
}

#[test]
fn edit_history_undoes_and_redoes_rotation_and_clears_dirty_at_identity() {
    let mut doc = dummy_document(100, 60);
    let mut history = EditHistory::default();

    assert!(!doc.is_dirty());
    assert!(!doc.has_unbaked_ops());

    history.apply(
        EditCommand::RotateCw {
            prev_annotations: Arc::from([]),
        },
        &mut doc,
    );
    assert!(doc.is_dirty());
    assert!(doc.has_unbaked_ops());
    assert_eq!(doc.transform.spec.rotation, OrthogonalRotation::Deg90);
    assert_eq!(doc.output_dimensions(), (60, 100));

    let mut fsm = ToolFsm::default();
    fsm.set_tool(ToolMode::Crop);
    fsm.begin_drag(Point2::new(10.0, 20.0));
    fsm.update_drag(Point2::new(40.0, 60.0));
    let outcome = fsm.finish_drag(&mut doc, &mut history);
    assert_eq!(
        outcome,
        FinishOutcome::CropStaged(PixelRect::new(10, 20, 30, 40))
    );
    assert!(fsm.apply_staged_crop(&mut doc, &mut history));
    assert_eq!(doc.output_dimensions(), (30, 40));

    assert!(history.undo(&mut doc));
    assert_eq!(doc.output_dimensions(), (60, 100));
    assert!(doc.is_dirty());
    assert!(history.undo(&mut doc));
    assert_eq!(doc.output_dimensions(), (100, 60));
    assert!(!doc.is_dirty());
    assert!(!history.undo(&mut doc));

    assert!(history.redo(&mut doc));
    assert_eq!(doc.output_dimensions(), (60, 100));
    assert!(doc.is_dirty());
    assert!(history.redo(&mut doc));
    assert_eq!(doc.output_dimensions(), (30, 40));

    // Four full clockwise rotations return to identity without leaving sticky dirty flag.
    let mut clean_doc = dummy_document(40, 20);
    let mut rot_history = EditHistory::default();
    for _ in 0..4 {
        rot_history.apply(
            EditCommand::RotateCw {
                prev_annotations: Arc::from([]),
            },
            &mut clean_doc,
        );
    }
    assert!(!clean_doc.has_unbaked_ops());
    assert!(!clean_doc.is_dirty());
}

#[test]
fn tool_fsm_drag_rejects_off_canvas_arrows_and_preserves_annotation_ids() {
    let mut doc = dummy_document(120, 90);
    let mut history = EditHistory::default();
    let mut fsm = ToolFsm::default();

    // Dragging an arrow completely outside the canvas must neither commit nor burn an ID.
    fsm.set_tool(ToolMode::Arrow);
    fsm.begin_drag(Point2::new(-50.0, 10.0));
    fsm.update_drag(Point2::new(-10.0, 70.0));
    assert_eq!(fsm.finish_drag(&mut doc, &mut history), FinishOutcome::None);
    assert_eq!(doc.next_annotation_id, 1);

    // Dragging a line across the boundary clips cleanly to the canvas rectangle.
    fsm.set_tool(ToolMode::Line);
    fsm.begin_drag(Point2::new(-20.0, 45.0));
    fsm.update_drag(Point2::new(60.0, 45.0));
    assert_eq!(
        fsm.finish_drag(&mut doc, &mut history),
        FinishOutcome::Committed
    );
    assert_eq!(doc.next_annotation_id, 2);
    match &doc.annotations[0].kind {
        AnnotationKind::Line { start, end, .. } => {
            assert_eq!(*start, [0.0, 45.0]);
            assert_eq!(*end, [60.0, 45.0]);
        }
        other => panic!("unexpected annotation variant: {other:?}"),
    }

    fsm.set_tool(ToolMode::Text);
    fsm.begin_drag(Point2::new(25.0, 35.0));
    assert!(fsm.commit_text("Callout #1", &mut doc, &mut history));
    assert_eq!(doc.annotations.len(), 2);

    fsm.set_tool(ToolMode::Blur);
    fsm.begin_drag(Point2::new(5.0, 5.0));
    assert!(fsm.cancel_active());
    assert_eq!(fsm.finish_drag(&mut doc, &mut history), FinishOutcome::None);

    assert!(history.undo(&mut doc));
    assert_eq!(doc.annotations.len(), 1);
    assert!(history.undo(&mut doc));
    assert!(doc.annotations.is_empty());
    assert!(!doc.is_dirty());
}

#[test]
fn edit_history_enforces_max_undo_depth() {
    let mut doc = dummy_document(10, 10);
    let mut history = EditHistory::default();

    for _ in 0..(MAX_UNDO_DEPTH + 10) {
        history.apply(
            EditCommand::FlipHorizontal {
                prev_annotations: Arc::from([]),
            },
            &mut doc,
        );
    }

    let mut undone = 0;
    while history.undo(&mut doc) {
        undone += 1;
    }
    assert_eq!(undone, MAX_UNDO_DEPTH);
}
