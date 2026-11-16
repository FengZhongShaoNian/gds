use gds::service::image::{
    PixelRect,
    ops::{OrthogonalRotation, TransformSpec, apply_transform},
};
use image::{Rgba, RgbaImage};

use super::TransformState;

fn unique_pixel_grid(width: u32, height: u32) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            img.put_pixel(x, y, Rgba([x as u8, y as u8, ((x + y) % 251) as u8, 255]));
        }
    }
    img
}

#[test]
fn map_output_rect_inverts_all_rotations_and_flips_against_apply_transform() {
    let base = unique_pixel_grid(12, 8);
    let rotations = [
        OrthogonalRotation::Deg0,
        OrthogonalRotation::Deg90,
        OrthogonalRotation::Deg180,
        OrthogonalRotation::Deg270,
    ];

    for rotation in rotations {
        for flip_h in [false, true] {
            for flip_v in [false, true] {
                let tf = TransformState {
                    spec: TransformSpec {
                        rotation,
                        flip_h,
                        flip_v,
                        crop: None,
                        resize: None,
                    },
                };
                let transformed = apply_transform(&base, &tf.spec).expect("apply transform");
                let (out_w, out_h) = tf.output_dimensions(12, 8);
                assert_eq!((transformed.width(), transformed.height()), (out_w, out_h));

                let selection = PixelRect::new(1, 2, out_w - 3, out_h - 4);
                let base_crop = tf
                    .map_output_rect_to_base_crop(selection, 12, 8)
                    .expect("mapped base crop");

                let cropped_then_transformed = apply_transform(
                    &base,
                    &TransformSpec {
                        rotation,
                        flip_h,
                        flip_v,
                        crop: Some(base_crop),
                        resize: None,
                    },
                )
                .expect("crop transform");

                assert_eq!(
                    (
                        cropped_then_transformed.width(),
                        cropped_then_transformed.height()
                    ),
                    (selection.width, selection.height)
                );
                for y in 0..selection.height {
                    for x in 0..selection.width {
                        assert_eq!(
                            cropped_then_transformed.get_pixel(x, y),
                            transformed.get_pixel(selection.x + x, selection.y + y),
                            "mismatch for rot={rotation:?}, flip_h={flip_h}, flip_v={flip_v} at ({x},{y})"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn map_output_rect_handles_stacked_crops_and_resize() {
    let base = unique_pixel_grid(20, 10);
    let tf = TransformState {
        spec: TransformSpec {
            rotation: OrthogonalRotation::Deg90,
            flip_h: true,
            flip_v: false,
            crop: Some(PixelRect::new(4, 2, 12, 6)),
            resize: None,
        },
    };
    let intermediate = apply_transform(&base, &tf.spec).expect("intermediate");
    // 12x6 rotated 90 deg -> 6x12 output.
    let sub_selection = PixelRect::new(1, 3, 4, 7);
    let stacked_crop = tf
        .map_output_rect_to_base_crop(sub_selection, 20, 10)
        .expect("stacked crop");
    let expected = apply_transform(
        &base,
        &TransformSpec {
            crop: Some(stacked_crop),
            ..tf.spec
        },
    )
    .expect("re-applied stacked crop");
    for y in 0..sub_selection.height {
        for x in 0..sub_selection.width {
            assert_eq!(
                expected.get_pixel(x, y),
                intermediate.get_pixel(sub_selection.x + x, sub_selection.y + y)
            );
        }
    }

    // Downscale 2x: 100x50 -> 50x25 output; selecting [10..30, 5..15] maps to [20..60, 10..30].
    let resized = TransformState {
        spec: TransformSpec {
            rotation: OrthogonalRotation::Deg0,
            flip_h: false,
            flip_v: false,
            crop: None,
            resize: Some((50, 25)),
        },
    };
    assert_eq!(resized.output_dimensions(100, 50), (50, 25));
    assert_eq!(
        resized.map_output_rect_to_base_crop(PixelRect::new(10, 5, 20, 10), 100, 50),
        Some(PixelRect::new(20, 10, 40, 20))
    );

    // Upscale 2x: 20x10 rotated 90 -> 10x20 -> resized to 20x40; selecting [2..8, 4..12] maps
    // back cleanly into base coordinates.
    let rotated_resized = TransformState {
        spec: TransformSpec {
            rotation: OrthogonalRotation::Deg90,
            flip_h: false,
            flip_v: false,
            crop: None,
            resize: Some((20, 40)),
        },
    };
    assert_eq!(
        rotated_resized.map_output_rect_to_base_crop(PixelRect::new(2, 4, 6, 8), 20, 10),
        Some(PixelRect::new(2, 6, 4, 3))
    );
}
