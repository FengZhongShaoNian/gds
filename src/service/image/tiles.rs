//! Tile geometry slicer for oversized images and long scrolling screenshots.

#[cfg(test)]
mod tests;

/// Default maximum tile edge when GPU texture limit is unknown (`8192px` is universal).
pub const DEFAULT_MAX_TILE_EDGE: u32 = 8192;

/// Single non-overlapping tile positioned in full document/canvas pixel coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileSpec {
    pub col: u32,
    pub row: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl TileSpec {
    /// Extracts this tile directly into a compact straight `BGRA8` buffer ready for GPU upload,
    /// avoiding duplicating the entire oversized source image.
    ///
    /// Returns an empty `0×0` buffer if the tile rectangle is empty or out of bounds.
    pub fn extract_bgra(&self, source_rgba: &image::RgbaImage) -> image::RgbaImage {
        let (src_w, src_h) = source_rgba.dimensions();
        let Some(end_x) = self.x.checked_add(self.width) else {
            return image::RgbaImage::new(0, 0);
        };
        let Some(end_y) = self.y.checked_add(self.height) else {
            return image::RgbaImage::new(0, 0);
        };
        if self.width == 0 || self.height == 0 || end_x > src_w || end_y > src_h {
            return image::RgbaImage::new(0, 0);
        }

        let Some(row_bytes) = (self.width as usize).checked_mul(4) else {
            return image::RgbaImage::new(0, 0);
        };
        let Some(total_bytes) = row_bytes.checked_mul(self.height as usize) else {
            return image::RgbaImage::new(0, 0);
        };
        let src_stride = (src_w as usize).saturating_mul(4);
        let start_x_byte = (self.x as usize).saturating_mul(4);
        let src_raw = source_rgba.as_raw();
        let mut buf = Vec::with_capacity(total_bytes);

        for row in 0..self.height as usize {
            let src_offset = (self.y as usize + row) * src_stride + start_x_byte;
            let src_row = &src_raw[src_offset..src_offset + row_bytes];
            buf.extend(
                src_row
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|px| [px[2], px[1], px[0], px[3]]),
            );
        }
        image::RgbaImage::from_raw(self.width, self.height, buf)
            .unwrap_or_else(|| image::RgbaImage::new(0, 0))
    }
}

/// Complete tile partition covering `[0..full_width, 0..full_height]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileGrid {
    pub full_width: u32,
    pub full_height: u32,
    pub max_tile_edge: u32,
    pub cols: u32,
    pub rows: u32,
    pub tiles: Vec<TileSpec>,
}

impl TileGrid {
    /// Computes the minimal tile partition where no tile exceeds `max_tile_edge` on either axis.
    pub fn compute(width: u32, height: u32, max_tile_edge: u32) -> Self {
        let edge = max_tile_edge.max(1);
        if width == 0 || height == 0 {
            return Self {
                full_width: width,
                full_height: height,
                max_tile_edge: edge,
                cols: 0,
                rows: 0,
                tiles: Vec::new(),
            };
        }
        let cols = width.div_ceil(edge);
        let rows = height.div_ceil(edge);
        let mut tiles =
            Vec::with_capacity((cols as usize).saturating_mul(rows as usize).min(1_048_576));
        for row in 0..rows {
            let y = row * edge;
            let h = (height - y).min(edge);
            for col in 0..cols {
                let x = col * edge;
                let w = (width - x).min(edge);
                tiles.push(TileSpec {
                    col,
                    row,
                    x,
                    y,
                    width: w,
                    height: h,
                });
            }
        }
        Self {
            full_width: width,
            full_height: height,
            max_tile_edge: edge,
            cols,
            rows,
            tiles,
        }
    }

    pub const fn needs_tiling(&self) -> bool {
        self.tiles.len() > 1
    }
}
