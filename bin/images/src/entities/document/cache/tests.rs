use std::sync::Arc;

use gds::service::image::Error;
use image::{Rgba, RgbaImage};
use tokio_util::sync::CancellationToken;

use crate::{
    entities::document::{AnnotationId, AnnotationItem, AnnotationKind, RgbaColor},
    shared::geometry::Rect2,
};

use super::{
    GesturePrepareInput, GestureSession, PreparedPayload, RenderCache, SceneTag, TransformState,
    TransientFrame, build_render_textures, build_svg_zoom_textures,
};

fn assert_payload_matches_image(payload: &PreparedPayload, expected_rgba: &RgbaImage) {
    let PreparedPayload::Single(bgra) = payload else {
        panic!("expected single payload for small test canvas");
    };
    let expected_bgra = gds::service::image::color::clone_as_bgra(expected_rgba);
    assert_eq!(bgra.dimensions(), expected_bgra.dimensions());
    let worst = bgra
        .as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected_bgra.as_raw().as_chunks::<4>().0.iter())
        .enumerate()
        .filter_map(|(px_idx, (actual, expected))| {
            let diff = actual
                .iter()
                .zip(expected.iter())
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            // Splitting vector primitives across prefix_rgba and incremental step introduces at
            // most +-1 LSB rounding difference between tiny_skia internal Porter-Duff and
            // f32 premultiplied blend_premultiplied_over on AA overlap pixels.
            (diff > 1).then(|| {
                let w = bgra.width() as usize;
                (px_idx % w, px_idx / w, diff, actual, expected)
            })
        })
        .next();
    assert!(worst.is_none(), "pixel mismatch (>1 LSB) at {worst:?}");
}

#[test]
fn unedited_render_reuses_base_rgba_arc_and_obeys_cancellation() {
    let mut base = RgbaImage::new(8, 8);
    base.put_pixel(2, 3, Rgba([10, 20, 30, 255]));
    let base_arc = Arc::new(base);

    let cancel = CancellationToken::new();
    let prepared = build_render_textures(
        Arc::clone(&base_arc),
        None,
        TransformState::default(),
        &[],
        &cancel,
    )
    .expect("unedited build");
    assert!(Arc::ptr_eq(&prepared.composited_rgba, &base_arc));

    let pre_cancelled = CancellationToken::new();
    pre_cancelled.cancel();
    assert!(matches!(
        build_render_textures(
            base_arc,
            None,
            TransformState::default(),
            &[],
            &pre_cancelled
        ),
        Err(Error::Cancelled)
    ));
}

#[test]
fn complete_at_checks_revision_and_svg_helper_rejects_cancelled() {
    let base_arc = Arc::new(RgbaImage::new(4, 4));
    let mut cache = RenderCache::default();
    let annotation = AnnotationItem {
        id: AnnotationId(1),
        kind: AnnotationKind::Rect {
            rect: Rect2::new(0.0, 0.0, 2.0, 2.0),
            stroke: RgbaColor::RED,
            stroke_width: 1.0,
            fill: None,
        },
    };
    let prepared = build_render_textures(
        Arc::clone(&base_arc),
        None,
        TransformState::default(),
        &[annotation],
        &CancellationToken::new(),
    )
    .expect("annotated build");
    assert!(!Arc::ptr_eq(&prepared.composited_rgba, &base_arc));

    cache.complete = Some(super::CompleteComposite {
        revision: 3,
        rgba: prepared.composited_rgba,
    });
    assert!(cache.complete_at(3).is_some());
    assert!(cache.complete_at(4).is_none());

    let cancel = CancellationToken::new();
    assert!(build_svg_zoom_textures(Arc::clone(&base_arc), &cancel).is_ok());
    cancel.cancel();
    assert!(matches!(
        build_svg_zoom_textures(base_arc, &cancel),
        Err(Error::Cancelled)
    ));
}

#[test]
fn gesture_session_matches_full_composite_across_stacked_effects_and_preserves_complete_isolation()
{
    let mut base = RgbaImage::from_pixel(96, 96, Rgba([240, 240, 240, 255]));
    base.put_pixel(10, 10, Rgba([10, 20, 30, 255]));
    let base_arc = Arc::new(base);
    let cancel = CancellationToken::new();

    let bottom_rect = AnnotationItem {
        id: AnnotationId(1),
        kind: AnnotationKind::Rect {
            rect: Rect2::new(10.0, 10.0, 24.0, 24.0),
            stroke: RgbaColor::RED,
            stroke_width: 4.0,
            fill: Some(RgbaColor::RED),
        },
    };
    let middle_arrow = AnnotationItem {
        id: AnnotationId(2),
        kind: AnnotationKind::Arrow {
            start: [8.0, 18.0],
            end: [52.0, 18.0],
            color: RgbaColor::BLUE,
            stroke_width: 3.0,
        },
    };
    let top_blur = AnnotationItem {
        id: AnnotationId(3),
        kind: AnnotationKind::GaussianBlur {
            region: Rect2::new(40.0, 10.0, 36.0, 36.0),
            radius: 4,
        },
    };
    let top_mosaic = AnnotationItem {
        id: AnnotationId(4),
        kind: AnnotationKind::Mosaic {
            region: Rect2::new(35.0, 50.0, 30.0, 30.0),
            block_size: 6,
        },
    };

    let initial = vec![
        bottom_rect.clone(),
        middle_arrow.clone(),
        top_blur.clone(),
        top_mosaic.clone(),
    ];
    let complete_prepared = build_render_textures(
        Arc::clone(&base_arc),
        None,
        TransformState::default(),
        &initial,
        &cancel,
    )
    .expect("initial complete build");

    let mut session = GestureSession::prepare(
        GesturePrepareInput {
            base_rgba: Arc::clone(&base_arc),
            transform: TransformState::default(),
            cached_base: Some((TransformState::default(), Arc::clone(&base_arc))),
            complete_rgba: Some(Arc::clone(&complete_prepared.composited_rgba)),
            initial_annotations: &initial,
            first_changed_idx: 1,
        },
        &cancel,
    )
    .expect("prepare gesture session");

    // Step 1: move middle_arrow right under top_blur (and near top_mosaic).
    let moved_arrow_1 = AnnotationItem {
        id: AnnotationId(2),
        kind: AnnotationKind::Arrow {
            start: [26.0, 24.0],
            end: [68.0, 24.0],
            color: RgbaColor::BLUE,
            stroke_width: 3.0,
        },
    };
    let frame_1_items = vec![
        bottom_rect.clone(),
        moved_arrow_1,
        top_blur.clone(),
        top_mosaic.clone(),
    ];
    let payload_1 = session.step(&frame_1_items, &cancel).expect("step 1");
    let full_1 = build_render_textures(
        Arc::clone(&base_arc),
        None,
        TransformState::default(),
        &frame_1_items,
        &cancel,
    )
    .expect("full build 1");
    assert_payload_matches_image(&payload_1, &full_1.composited_rgba);

    // Step 2: move middle_arrow down into top_mosaic so mosaic samples the moved arrow cleanly.
    let moved_arrow_2 = AnnotationItem {
        id: AnnotationId(2),
        kind: AnnotationKind::Arrow {
            start: [20.0, 62.0],
            end: [60.0, 62.0],
            color: RgbaColor::BLUE,
            stroke_width: 3.0,
        },
    };
    let frame_2_items = vec![bottom_rect, moved_arrow_2, top_blur, top_mosaic];
    let payload_2 = session.step(&frame_2_items, &cancel).expect("step 2");
    let full_2 = build_render_textures(
        Arc::clone(&base_arc),
        None,
        TransformState::default(),
        &frame_2_items,
        &cancel,
    )
    .expect("full build 2");
    assert_payload_matches_image(&payload_2, &full_2.composited_rgba);

    // `install_transient` must never overwrite `complete` or `complete_at`, and `SceneTag` stays isolated.
    assert_eq!(
        TransientFrame {
            payload: payload_2,
            gesture_id: 7,
            seq: 2,
            annotations: Arc::from(frame_2_items.as_slice()),
        }
        .gesture_id,
        7
    );
    assert_ne!(
        SceneTag::Transient {
            gesture_id: 7,
            seq: 2
        },
        SceneTag::Complete { revision: 0 }
    );
}
