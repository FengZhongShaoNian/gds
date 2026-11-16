use std::sync::Arc;

use gds::service::image::Error;
use image::{Rgba, RgbaImage};
use tokio_util::sync::CancellationToken;

use crate::entities::document::{AnnotationId, AnnotationItem, AnnotationKind, RgbaColor};

use super::{RenderCache, TransformState, build_render_textures, build_svg_zoom_textures};

#[test]
fn unedited_render_reuses_base_rgba_arc_and_obeys_cancellation() {
    let mut base = RgbaImage::new(8, 8);
    base.put_pixel(2, 3, Rgba([10, 20, 30, 255]));
    let base_arc = Arc::new(base);

    let cancel = CancellationToken::new();
    let prepared = build_render_textures(
        Arc::clone(&base_arc),
        TransformState::default(),
        &[],
        &cancel,
    )
    .expect("unedited build");
    assert!(Arc::ptr_eq(&prepared.composited_rgba, &base_arc));

    let pre_cancelled = CancellationToken::new();
    pre_cancelled.cancel();
    assert!(matches!(
        build_render_textures(base_arc, TransformState::default(), &[], &pre_cancelled),
        Err(Error::Cancelled)
    ));
}

#[test]
fn composited_at_checks_revision_and_svg_helper_rejects_cancelled() {
    let base_arc = Arc::new(RgbaImage::new(4, 4));
    let mut cache = RenderCache::default();
    let annotation = AnnotationItem {
        id: AnnotationId(1),
        kind: AnnotationKind::Rect {
            rect: gds::service::image::PixelRect::new(0, 0, 2, 2),
            stroke: RgbaColor::RED,
            stroke_width: 1.0,
            fill: None,
        },
    };
    let prepared = build_render_textures(
        Arc::clone(&base_arc),
        TransformState::default(),
        &[annotation],
        &CancellationToken::new(),
    )
    .expect("annotated build");
    assert!(!Arc::ptr_eq(&prepared.composited_rgba, &base_arc));

    cache.composited = Some(prepared.composited_rgba);
    cache.composited_revision = 3;
    assert!(cache.composited_at(3).is_some());
    assert!(cache.composited_at(4).is_none());

    let cancel = CancellationToken::new();
    assert!(build_svg_zoom_textures(Arc::clone(&base_arc), &cancel).is_ok());
    cancel.cancel();
    assert!(matches!(
        build_svg_zoom_textures(base_arc, &cancel),
        Err(Error::Cancelled)
    ));
}
