use gds::service::image::PixelRect;

use super::{FitMode, MIN_ZOOM, Point2, Rect2, Size2, ViewportTransform, clip_segment_to_rect};

#[test]
fn fit_modes_center_and_scale_accurately() {
    let viewport = Size2::new(800.0, 600.0);
    let image = Size2::new(1600.0, 800.0);

    let fit = ViewportTransform::compute_fit(FitMode::FitToWindow, viewport, image);
    assert!((fit.scale - 0.5).abs() < 1e-6);
    assert!((fit.pan_x - 0.0).abs() < 1e-6);
    assert!((fit.pan_y - 100.0).abs() < 1e-6);

    let actual = ViewportTransform::compute_fit(FitMode::ActualPixels, viewport, image);
    assert!((actual.scale - 1.0).abs() < 1e-6);
    assert!((actual.pan_x - (-400.0)).abs() < 1e-6);
    assert!((actual.pan_y - (-100.0)).abs() < 1e-6);

    let fill_h = ViewportTransform::compute_fit(FitMode::FillHeight, viewport, image);
    assert!((fill_h.scale - 0.75).abs() < 1e-6);
    assert!((fill_h.pan_x - (-200.0)).abs() < 1e-6);
}

#[test]
fn cursor_anchored_zoom_preserves_image_point_under_cursor() {
    let viewport = Size2::new(1000.0, 800.0);
    let image = Size2::new(2000.0, 1000.0);
    let mut tf = ViewportTransform::compute_fit(FitMode::FitToWindow, viewport, image);
    let cursor = Point2::new(640.0, 320.0);

    let before = tf.viewport_to_image(cursor);
    tf.zoom_around_cursor(1.85, cursor);
    let after = tf.viewport_to_image(cursor);

    assert_eq!(tf.fit_mode, FitMode::Custom);
    assert!((before.x - after.x).abs() < 1e-3);
    assert!((before.y - after.y).abs() < 1e-3);

    let roundtrip = tf.image_to_viewport(after);
    assert!((roundtrip.x - cursor.x).abs() < 1e-3);
    assert!((roundtrip.y - cursor.y).abs() < 1e-3);
}

#[test]
fn drag_rect_normalization_and_pixel_clamping() {
    let rect =
        ViewportTransform::normalize_drag_rect(Point2::new(120.4, 90.8), Point2::new(-15.0, 30.2));
    assert!((rect.x - (-15.0)).abs() < 1e-6);
    assert!((rect.y - 30.2).abs() < 1e-6);
    assert!((rect.width - 135.4).abs() < 1e-4);

    let clamped = ViewportTransform::clamp_image_rect(
        Rect2::new(-5.0, 10.2, 50.0, 40.0),
        Size2::new(40.0, 35.0),
    )
    .expect("clamped pixel rect");
    assert_eq!(clamped.x, 0);
    assert_eq!(clamped.y, 10);
    assert_eq!(clamped.width, 40);
    assert_eq!(clamped.height, 25);
}

#[test]
fn non_finite_zoom_and_zero_scale_edges_stay_valid() {
    let mut tf = ViewportTransform::compute_fit(
        FitMode::FitToWindow,
        Size2::new(800.0, 600.0),
        Size2::new(1600.0, 800.0),
    );
    let before = tf;
    tf.zoom_around_cursor(f32::NAN, Point2::new(100.0, 100.0));
    tf.zoom_around_cursor(f32::INFINITY, Point2::new(100.0, 100.0));
    tf.zoom_around_cursor(0.0, Point2::new(100.0, 100.0));
    assert_eq!(tf, before);

    tf.scale = 0.0;
    let clamped_vp = tf.image_to_viewport(Point2::new(100.0, 50.0));
    assert!((clamped_vp.x - (100.0 * MIN_ZOOM + tf.pan_x)).abs() < 1e-6);
    assert!(
        (tf.image_rect_to_viewport(Rect2::new(0.0, 0.0, 10.0, 20.0))
            .width
            - 10.0 * MIN_ZOOM)
            .abs()
            < 1e-6
    );

    tf.set_scale_centered(1.5, Size2::new(800.0, 600.0));
    assert!((tf.scale - 1.5).abs() < 1e-6);
    tf.set_scale_centered(f32::NAN, Size2::new(800.0, 600.0));
    assert!((tf.scale - 1.5).abs() < 1e-6);
}

#[test]
fn rect2_helpers_and_segment_clipping_handle_signed_and_crossing_geometry() {
    let raw = Rect2::new(30.0, 40.0, -20.0, -10.0);
    let norm = raw.normalized();
    assert_eq!(norm, Rect2::new(10.0, 30.0, 20.0, 10.0));
    assert_eq!(norm.right(), 30.0);
    assert_eq!(norm.bottom(), 40.0);
    assert_eq!(norm.center(), Point2::new(20.0, 35.0));

    let expanded = norm.expand(2.5);
    assert_eq!(expanded, Rect2::new(7.5, 27.5, 25.0, 15.0));
    assert!(expanded.contains(Point2::new(7.5, 27.5)));
    assert!(!expanded.contains(Point2::new(7.4, 27.5)));

    let other = Rect2::new(25.0, 35.0, 15.0, 15.0);
    assert_eq!(norm.union(other), Rect2::new(10.0, 30.0, 30.0, 20.0));
    assert_eq!(
        norm.intersect(other),
        Some(Rect2::new(25.0, 35.0, 5.0, 5.0))
    );
    assert!(norm.intersects_inclusive(Rect2::new(30.0, 40.0, 5.0, 5.0)));
    assert!(!norm.intersects_inclusive(Rect2::new(30.1, 40.0, 5.0, 5.0)));

    let partial = Rect2::new(-5.2, 10.4, 15.5, 20.3);
    assert_eq!(
        partial.to_clamped_pixel_rect(100, 100),
        Some(PixelRect::new(0, 10, 11, 21))
    );
    assert_eq!(
        Rect2::new(-20.0, 0.0, 10.0, 10.0).to_clamped_pixel_rect(100, 100),
        None
    );

    let box_rect = Rect2::new(0.0, 0.0, 100.0, 80.0);
    let clipped =
        clip_segment_to_rect(Point2::new(-50.0, 40.0), Point2::new(150.0, 40.0), box_rect)
            .expect("crossing segment clips");
    assert_eq!(clipped, (Point2::new(0.0, 40.0), Point2::new(100.0, 40.0)));
    assert_eq!(
        clip_segment_to_rect(
            Point2::new(0.0, 100.0),
            Point2::new(100.0, 0.0),
            Rect2::new(0.0, 0.0, 20.0, 20.0)
        ),
        None
    );
}
