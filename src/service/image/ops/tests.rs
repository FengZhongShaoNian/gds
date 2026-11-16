use image::{Rgba, RgbaImage};

use super::*;

fn solid_canvas(width: u32, height: u32, color: [u8; 4]) -> RgbaImage {
    RgbaImage::from_pixel(width, height, Rgba(color))
}

#[test]
fn gaussian_blur_avoids_dark_transparent_halo_and_preserves_uniform_regions() {
    // Pure red opaque square surrounded by transparent black `[0, 0, 0, 0]`.
    let mut img = solid_canvas(32, 32, [0, 0, 0, 0]);
    for y in 12..20 {
        for x in 12..20 {
            img.put_pixel(x, y, Rgba([220, 40, 20, 255]));
        }
    }

    apply_gaussian_blur(&mut img, PixelRect::new(8, 8, 16, 16), 3);

    // Unblurred border outside the region must remain transparent black.
    assert_eq!(img.get_pixel(0, 0).0, [0, 0, 0, 0]);

    // Feathered boundary pixels just outside the red square gain partial alpha while keeping pure red color.
    let edge = img.get_pixel(11, 16).0;
    assert!(
        edge[3] > 0 && edge[3] < 255,
        "edge pixel should feather: {edge:?}"
    );
    assert!(
        edge[0] >= 210 && edge[1] <= 45 && edge[2] <= 25,
        "straight RGB must not darken toward [0,0,0]: {edge:?}"
    );

    // Uniform opaque region must remain exact through 3-pass separable blur.
    let mut uniform = solid_canvas(24, 24, [120, 180, 240, 255]);
    apply_gaussian_blur(&mut uniform, PixelRect::new(2, 2, 20, 20), 6);
    assert_eq!(uniform.get_pixel(12, 12).0, [120, 180, 240, 255]);
}

#[test]
fn mosaic_alpha_weights_rgb_and_solid_fill_alpha_blends() {
    let mut img = solid_canvas(4, 4, [0, 0, 0, 0]);
    img.put_pixel(0, 0, Rgba([200, 100, 50, 255]));
    apply_mosaic(&mut img, PixelRect::new(0, 0, 2, 2), 2);
    // 1 opaque [200,100,50,255] + 3 transparent [0,0,0,0] -> straight RGB remains [200,100,50], alpha ~ 64.
    let p = img.get_pixel(1, 1).0;
    assert_eq!(p[0], 200);
    assert_eq!(p[1], 100);
    assert_eq!(p[2], 50);
    assert!(p[3] > 60 && p[3] < 68);

    // Fully transparent block stays `[0, 0, 0, 0]`.
    apply_mosaic(&mut img, PixelRect::new(2, 2, 2, 2), 2);
    assert_eq!(img.get_pixel(2, 2).0, [0, 0, 0, 0]);

    // Translucent solid fill source-over blends onto opaque white; zero-alpha is a no-op.
    let mut bg = solid_canvas(4, 4, [255, 255, 255, 255]);
    apply_solid_fill(&mut bg, PixelRect::new(1, 1, 2, 2), [0, 0, 255, 0]);
    assert_eq!(bg.get_pixel(1, 1).0, [255, 255, 255, 255]);
    apply_solid_fill(&mut bg, PixelRect::new(1, 1, 2, 2), [0, 0, 255, 128]);
    let blended = bg.get_pixel(1, 1).0;
    assert!(blended[0] > 115 && blended[0] < 140);
    assert!(blended[2] == 255);
    assert_eq!(blended[3], 255);
}

#[test]
fn composite_ops_renders_dirty_bbox_strips_xml_control_chars_and_handles_edge_geometries() {
    let mut canvas = solid_canvas(240, 200, [255, 255, 255, 255]);
    let ops = vec![
        // Negative width/height normalized into [160..200, 140..180].
        RasterOp::Rect {
            x: 200.0,
            y: 180.0,
            width: -40.0,
            height: -40.0,
            color: [255, 0, 0, 255],
            stroke_width: 2.0,
            fill: Some([0, 200, 0, 255]),
        },
        // Short arrow stays bounded without backward overshoot.
        RasterOp::Arrow {
            start: [10.0, 100.0],
            end: [20.0, 100.0],
            color: [0, 0, 255, 255],
            stroke_width: 4.0,
        },
        // Degenerate zero-length and non-finite ops are safely skipped.
        RasterOp::Arrow {
            start: [50.0, 50.0],
            end: [50.0, 50.0],
            color: [0, 0, 0, 255],
            stroke_width: 4.0,
        },
        RasterOp::Line {
            start: [f32::NAN, 10.0],
            end: [20.0, 10.0],
            color: [0, 0, 0, 255],
            stroke_width: 2.0,
        },
        // XML 1.0 control characters stripped from text cleanly.
        RasterOp::Text {
            position: [20.0, 20.0],
            content: "OK\x00\x07\x1b!".to_owned(),
            font_size: 16.0,
            color: [10, 10, 10, 255],
        },
        // Step badge with explicit transparent text alpha leaves badge fill visible cleanly.
        RasterOp::StepBadge {
            center: [120.0, 40.0],
            number: 1,
            radius: 12.0,
            fill: [255, 0, 0, 255],
            text_color: [255, 255, 255, 0],
        },
    ];

    composite_ops(&mut canvas, &ops).expect("composite ops with edge geometries");

    // Inside normalized green filled rect [160..200, 140..180].
    assert_eq!(canvas.get_pixel(180, 160).0, [0, 200, 0, 255]);
    // Step badge center has transparent text_color[3]=0 so red fill must show through unmasked.
    assert_eq!(canvas.get_pixel(120, 40).0, [255, 0, 0, 255]);
    // Area away from all ops remains untouched white.
    assert_eq!(canvas.get_pixel(230, 10).0, [255, 255, 255, 255]);
    // Area where degenerate NaN/zero-len arrow sat remains untouched white.
    assert_eq!(canvas.get_pixel(50, 50).0, [255, 255, 255, 255]);
}

#[test]
fn apply_transform_covers_crop_rotations_flips_and_validation() {
    let mut img = solid_canvas(4, 2, [0, 0, 0, 255]);
    img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    img.put_pixel(3, 1, Rgba([0, 255, 0, 255]));

    let rot180 = apply_transform(
        &img,
        &TransformSpec {
            rotation: OrthogonalRotation::Deg180,
            ..Default::default()
        },
    )
    .expect("rotate 180");
    assert_eq!(rot180.get_pixel(3, 1).0, [255, 0, 0, 255]);
    assert_eq!(rot180.get_pixel(0, 0).0, [0, 255, 0, 255]);

    let cropped = apply_transform(
        &img,
        &TransformSpec {
            crop: Some(PixelRect::new(0, 0, 2, 1)),
            rotation: OrthogonalRotation::Deg90,
            ..Default::default()
        },
    )
    .expect("crop + rotate 90");
    assert_eq!(cropped.dimensions(), (1, 2));

    assert!(
        apply_transform(
            &img,
            &TransformSpec {
                crop: Some(PixelRect::new(10, 10, 2, 2)),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        apply_transform(
            &img,
            &TransformSpec {
                resize: Some((0, 4)),
                ..Default::default()
            }
        )
        .is_err()
    );
}
