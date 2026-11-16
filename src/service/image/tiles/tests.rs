use image::{Rgba, RgbaImage};

use super::*;

#[test]
fn tile_grid_partitions_canvas_cleanly_and_handles_empty_edges() {
    let empty = TileGrid::compute(0, 100, 64);
    assert_eq!(empty.cols, 0);
    assert_eq!(empty.rows, 0);
    assert!(empty.tiles.is_empty());
    assert!(!empty.needs_tiling());

    let single = TileGrid::compute(100, 80, 128);
    assert_eq!(single.cols, 1);
    assert_eq!(single.rows, 1);
    assert!(!single.needs_tiling());
    assert_eq!(
        single.tiles[0],
        TileSpec {
            col: 0,
            row: 0,
            x: 0,
            y: 0,
            width: 100,
            height: 80,
        }
    );

    let multi = TileGrid::compute(10, 7, 4);
    assert_eq!(multi.cols, 3);
    assert_eq!(multi.rows, 2);
    assert!(multi.needs_tiling());
    assert_eq!(multi.tiles.len(), 6);
    assert_eq!(
        multi.tiles[0],
        TileSpec {
            col: 0,
            row: 0,
            x: 0,
            y: 0,
            width: 4,
            height: 4
        }
    );
    assert_eq!(
        multi.tiles[2],
        TileSpec {
            col: 2,
            row: 0,
            x: 8,
            y: 0,
            width: 2,
            height: 4
        }
    );
    assert_eq!(
        multi.tiles[5],
        TileSpec {
            col: 2,
            row: 1,
            x: 8,
            y: 4,
            width: 2,
            height: 3
        }
    );
}

#[test]
fn extract_bgra_swaps_red_blue_and_guards_out_of_bounds_tiles() {
    let mut src = RgbaImage::new(6, 5);
    src.put_pixel(3, 2, Rgba([10, 20, 30, 200]));
    src.put_pixel(4, 3, Rgba([250, 120, 5, 180]));

    let spec = TileSpec {
        col: 1,
        row: 1,
        x: 3,
        y: 2,
        width: 2,
        height: 2,
    };
    let bgra = spec.extract_bgra(&src);
    assert_eq!(bgra.dimensions(), (2, 2));
    assert_eq!(bgra.get_pixel(0, 0).0, [30, 20, 10, 200]);
    assert_eq!(bgra.get_pixel(1, 1).0, [5, 120, 250, 180]);

    // Out-of-bounds and u32::MAX overflow specs return empty `0x0` rather than panicking.
    let oob = TileSpec {
        col: 0,
        row: 0,
        x: 5,
        y: 2,
        width: 3,
        height: 2,
    };
    assert_eq!(oob.extract_bgra(&src).dimensions(), (0, 0));

    let overflow = TileSpec {
        col: 0,
        row: 0,
        x: u32::MAX - 1,
        y: 0,
        width: 4,
        height: 2,
    };
    assert_eq!(overflow.extract_bgra(&src).dimensions(), (0, 0));
}
