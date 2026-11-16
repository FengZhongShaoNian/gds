//! Isolated export cache (`CompleteComposite`), transient display frames (`PresentedScene`),
//! and incremental prefix/dirty-region `GestureSession`.

#[cfg(test)]
mod tests;

use std::sync::Arc;

use gds::service::image::{
    Error, PixelRect, color,
    ops::{RasterOp, apply_transform, composite_ops_clipped},
    tiles::{DEFAULT_MAX_TILE_EDGE, TileGrid, TileSpec},
};
use gpui_kit::{App, RenderImage};
use image::{Frame, RgbaImage};
use tokio_util::sync::CancellationToken;

use super::{
    annotation::{AnnotationItem, AnnotationKind},
    transform::TransformState,
};
use crate::shared::geometry::Rect2;

/// Authoritative RGBA8 pixels composited exclusively from a committed [`super::DocumentState`].
///
/// Export and clipboard pipelines may reuse ONLY this buffer when `revision == doc.edit_revision`.
#[derive(Clone)]
pub struct CompleteComposite {
    pub revision: u64,
    pub rgba: Arc<RgbaImage>,
}

/// Distinguishes committed document textures from live interactive gesture frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneTag {
    Complete { revision: u64 },
    Transient { gesture_id: u64, seq: u64 },
}

/// Currently presented GPU texture paired with the matching annotation snapshot for overlays.
#[derive(Clone)]
pub struct PresentedScene {
    pub output: RenderOutput,
    pub tag: SceneTag,
    /// When `Some`, overlays (selection bounds, handles) must read these coordinates instead of
    /// `doc.annotations` so handles remain frame-locked to the displayed GPU texture.
    pub transient_annotations: Option<Arc<[AnnotationItem]>>,
}

/// Off-thread output holding composited RGBA8 (for save/copy), cached base, and BGRA8 frames.
#[derive(Clone)]
pub struct PreparedTextures {
    pub composited_rgba: Arc<RgbaImage>,
    pub transformed_base: Arc<RgbaImage>,
    pub payload: PreparedPayload,
}

/// Transient frame descriptor installed via [`RenderCache::install_transient`].
#[derive(Clone)]
pub struct TransientFrame {
    pub payload: PreparedPayload,
    pub gesture_id: u64,
    pub seq: u64,
    pub annotations: Arc<[AnnotationItem]>,
}

#[derive(Clone)]
pub enum PreparedPayload {
    Single(RgbaImage),
    Tiled { tiles: Vec<(TileSpec, RgbaImage)> },
}

/// GPU-ready GPUI render images (either one surface or a tiled grid for giant images).
#[derive(Clone)]
pub enum RenderOutput {
    Single {
        image: Arc<RenderImage>,
    },
    Tiled {
        tiles: Vec<(TileSpec, Arc<RenderImage>)>,
    },
}

#[derive(Default)]
pub struct RenderCache {
    pub complete: Option<CompleteComposite>,
    pub presented: Option<PresentedScene>,
    pub transformed_base: Option<(TransformState, Arc<RgbaImage>)>,
    pub generation: u64,
}

impl RenderCache {
    pub fn complete_at(&self, revision: u64) -> Option<&Arc<RgbaImage>> {
        self.complete
            .as_ref()
            .filter(|entry| entry.revision == revision)
            .map(|entry| &entry.rgba)
    }

    pub fn presented_output(&self) -> Option<&RenderOutput> {
        self.presented.as_ref().map(|scene| &scene.output)
    }

    pub fn presented_annotations<'a>(
        &'a self,
        committed: &'a [AnnotationItem],
    ) -> &'a [AnnotationItem] {
        match &self.presented {
            Some(scene) => scene.transient_annotations.as_deref().unwrap_or(committed),
            None => committed,
        }
    }

    pub fn release_textures(&mut self, cx: &mut App) {
        let Some(old) = self.presented.take() else {
            return;
        };
        match old.output {
            RenderOutput::Single { image } => {
                cx.drop_image(image, None);
            }
            RenderOutput::Tiled { tiles } => {
                for (_, image) in tiles {
                    cx.drop_image(image, None);
                }
            }
        }
    }

    pub fn install_complete(
        &mut self,
        prepared: PreparedTextures,
        transform: TransformState,
        generation: u64,
        revision: u64,
        cx: &mut App,
    ) {
        self.release_textures(cx);
        self.transformed_base = Some((transform, prepared.transformed_base));
        self.complete = Some(CompleteComposite {
            revision,
            rgba: prepared.composited_rgba,
        });
        self.presented = Some(PresentedScene {
            output: upload_payload(prepared.payload),
            tag: SceneTag::Complete { revision },
            transient_annotations: None,
        });
        self.generation = generation;
    }

    /// Replaces only the presented GPU scene with a live gesture preview frame.
    ///
    /// Never alters [`Self::complete`] or [`Self::transformed_base`].
    pub fn install_transient(&mut self, frame: TransientFrame, generation: u64, cx: &mut App) {
        self.release_textures(cx);
        self.presented = Some(PresentedScene {
            output: upload_payload(frame.payload),
            tag: SceneTag::Transient {
                gesture_id: frame.gesture_id,
                seq: frame.seq,
            },
            transient_annotations: Some(frame.annotations),
        });
        self.generation = generation;
    }

    /// Replaces only the presented display texture (for SVG zoom re-rasterization or Esc
    /// cancellation back to `complete`) without altering `complete`.
    pub fn install_display_output(
        &mut self,
        payload: PreparedPayload,
        revision: u64,
        generation: u64,
        cx: &mut App,
    ) {
        self.release_textures(cx);
        self.presented = Some(PresentedScene {
            output: upload_payload(payload),
            tag: SceneTag::Complete { revision },
            transient_annotations: None,
        });
        self.generation = generation;
    }
}

fn upload_payload(payload: PreparedPayload) -> RenderOutput {
    match payload {
        PreparedPayload::Single(bgra) => RenderOutput::Single {
            image: Arc::new(RenderImage::new(vec![Frame::new(bgra)])),
        },
        PreparedPayload::Tiled { tiles } => RenderOutput::Tiled {
            tiles: tiles
                .into_iter()
                .map(|(spec, bgra)| (spec, Arc::new(RenderImage::new(vec![Frame::new(bgra)]))))
                .collect(),
        },
    }
}

/// Evaluates transforms + annotations on CPU and slices oversized canvases into Straight-BGRA8 tiles.
///
/// Reuses `base_rgba` directly via `Arc::clone` when no transform or annotation edits exist, and
/// reuses `cached_base` whenever `transform` matches.
///
/// # Errors
/// Returns [`Error::Cancelled`], [`Error::UnsupportedOperation`], [`Error::Svg`], or
/// [`Error::TooLarge`] if cancelled or rendering fails.
pub fn build_render_textures(
    base_rgba: Arc<RgbaImage>,
    cached_base: Option<(TransformState, Arc<RgbaImage>)>,
    transform: TransformState,
    annotations: &[AnnotationItem],
    cancel: &CancellationToken,
) -> Result<PreparedTextures, Error> {
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }

    let transformed_base = resolve_transformed_base(base_rgba, cached_base, transform)?;
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }

    let composited_rgba = if annotations.is_empty() {
        Arc::clone(&transformed_base)
    } else {
        let mut composited = (*transformed_base).clone();
        let (cw, ch) = composited.dimensions();
        let ops: Vec<RasterOp> = annotations
            .iter()
            .filter_map(|item| item.to_raster_op(cw, ch))
            .collect();
        composite_ops_clipped(&mut composited, &ops, None)?;
        Arc::new(composited)
    };

    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }

    let payload = encode_display_payload(&composited_rgba, cancel)?;
    Ok(PreparedTextures {
        composited_rgba,
        transformed_base,
        payload,
    })
}

/// Converts a zoomed SVG rasterization or cached `complete` buffer into Straight-BGRA8 display textures.
///
/// # Errors
/// Returns [`Error::Cancelled`] if superseded before upload preparation completes.
pub fn build_svg_zoom_textures(
    rgba: Arc<RgbaImage>,
    cancel: &CancellationToken,
) -> Result<PreparedPayload, Error> {
    encode_display_payload(&rgba, cancel)
}

/// Encodes Straight RGBA8 pixels into single or tiled Straight BGRA8 display buffers.
///
/// # Errors
/// Returns [`Error::Cancelled`] if cancelled mid-tile extraction.
pub fn encode_display_payload(
    composited: &RgbaImage,
    cancel: &CancellationToken,
) -> Result<PreparedPayload, Error> {
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let width = composited.width();
    let height = composited.height();
    let grid = TileGrid::compute(width, height, DEFAULT_MAX_TILE_EDGE);
    if grid.needs_tiling() {
        let mut tiles = Vec::with_capacity(grid.tiles.len());
        for &spec in &grid.tiles {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            tiles.push((spec, spec.extract_bgra(composited)));
        }
        Ok(PreparedPayload::Tiled { tiles })
    } else {
        Ok(PreparedPayload::Single(color::clone_as_bgra(composited)))
    }
}

/// Inputs required to prepare a prefix-cached [`GestureSession`].
pub struct GesturePrepareInput<'a> {
    pub base_rgba: Arc<RgbaImage>,
    pub transform: TransformState,
    pub cached_base: Option<(TransformState, Arc<RgbaImage>)>,
    pub complete_rgba: Option<Arc<RgbaImage>>,
    pub initial_annotations: &'a [AnnotationItem],
    pub first_changed_idx: usize,
}

/// Pure-CPU incremental compositor for interactive drag/resize/text gestures.
///
/// Freezes `0..first_changed_idx` into `prefix_rgba` and composites only dirty regions
/// across `first_changed_idx..` in authoritative document stacking order.
pub struct GestureSession {
    first_changed_idx: usize,
    canvas_w: u32,
    canvas_h: u32,
    prefix_rgba: Arc<RgbaImage>,
    work_rgba: RgbaImage,
    bgra_single: Option<RgbaImage>,
    cached_tiles: Vec<(TileSpec, RgbaImage)>,
    tile_grid: TileGrid,
    prev_annotations: Arc<[AnnotationItem]>,
    needs_full_first_pass: bool,
}

impl GestureSession {
    /// Freezes the immutable prefix `0..first_changed_idx` and initializes working buffers.
    ///
    /// # Errors
    /// Returns [`Error::Cancelled`], [`Error::UnsupportedOperation`], [`Error::Svg`], or
    /// [`Error::TooLarge`] if cancelled or prefix compositing fails.
    pub fn prepare(
        input: GesturePrepareInput<'_>,
        cancel: &CancellationToken,
    ) -> Result<Self, Error> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }

        let transformed_base =
            resolve_transformed_base(input.base_rgba, input.cached_base, input.transform)?;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }

        let (canvas_w, canvas_h) = transformed_base.dimensions();
        let clamped_idx = input.first_changed_idx.min(input.initial_annotations.len());
        let valid_complete = input
            .complete_rgba
            .filter(|img| img.dimensions() == (canvas_w, canvas_h));

        let prefix_rgba = if clamped_idx == 0 {
            Arc::clone(&transformed_base)
        } else if clamped_idx == input.initial_annotations.len()
            && let Some(complete) = &valid_complete
        {
            Arc::clone(complete)
        } else {
            let mut buf = (*transformed_base).clone();
            let ops: Vec<RasterOp> = input.initial_annotations[..clamped_idx]
                .iter()
                .filter_map(|item| item.to_raster_op(canvas_w, canvas_h))
                .collect();
            composite_ops_clipped(&mut buf, &ops, None)?;
            Arc::new(buf)
        };

        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }

        let (work_rgba, needs_full_first_pass) = if let Some(complete) = valid_complete {
            ((*complete).clone(), false)
        } else if clamped_idx == input.initial_annotations.len() {
            ((*prefix_rgba).clone(), false)
        } else {
            ((*prefix_rgba).clone(), true)
        };

        let tile_grid = TileGrid::compute(canvas_w, canvas_h, DEFAULT_MAX_TILE_EDGE);
        let (bgra_single, cached_tiles) = if tile_grid.needs_tiling() {
            let mut tiles = Vec::with_capacity(tile_grid.tiles.len());
            for &spec in &tile_grid.tiles {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                tiles.push((spec, spec.extract_bgra(&work_rgba)));
            }
            (None, tiles)
        } else {
            (Some(color::clone_as_bgra(&work_rgba)), Vec::new())
        };

        Ok(Self {
            first_changed_idx: clamped_idx,
            canvas_w,
            canvas_h,
            prefix_rgba,
            work_rgba,
            bgra_single,
            cached_tiles,
            tile_grid,
            prev_annotations: Arc::from(input.initial_annotations),
            needs_full_first_pass,
        })
    }

    /// Incrementally re-composites the minimal dirty region (including intersecting upper
    /// `Mosaic`/`GaussianBlur` regions) and returns an updated BGRA8 display payload.
    ///
    /// # Errors
    /// Returns [`Error::Cancelled`], [`Error::Svg`], or [`Error::TooLarge`].
    pub fn step(
        &mut self,
        next_annotations: &[AnnotationItem],
        cancel: &CancellationToken,
    ) -> Result<PreparedPayload, Error> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if self.canvas_w == 0 || self.canvas_h == 0 {
            return encode_display_payload(&self.work_rgba, cancel);
        }

        let clip_rect = if self.needs_full_first_pass {
            self.needs_full_first_pass = false;
            Some(PixelRect::new(0, 0, self.canvas_w, self.canvas_h))
        } else {
            self.compute_dirty_clip(next_annotations)
        };

        if let Some(clip) = clip_rect {
            copy_rgba_rect(&self.prefix_rgba, &mut self.work_rgba, clip);
            let tail = next_annotations
                .get(self.first_changed_idx..)
                .unwrap_or(&[]);
            let ops: Vec<RasterOp> = tail
                .iter()
                .filter_map(|item| item.to_raster_op(self.canvas_w, self.canvas_h))
                .collect();
            composite_ops_clipped(&mut self.work_rgba, &ops, Some(clip))?;
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            self.sync_bgra_rect(clip, cancel)?;
        }

        self.prev_annotations = Arc::from(next_annotations);
        self.build_payload()
    }

    fn compute_dirty_clip(&self, next_annotations: &[AnnotationItem]) -> Option<PixelRect> {
        let mut dirty: Option<Rect2> = None;
        let max_len = self.prev_annotations.len().max(next_annotations.len());
        for idx in self.first_changed_idx..max_len {
            let prev = self.prev_annotations.get(idx);
            let next = next_annotations.get(idx);
            if prev == next {
                continue;
            }
            if let Some(item) = prev {
                let bounds = item.dirty_bounds();
                dirty = Some(dirty.map_or(bounds, |acc| acc.union(bounds)));
            }
            if let Some(item) = next {
                let bounds = item.dirty_bounds();
                dirty = Some(dirty.map_or(bounds, |acc| acc.union(bounds)));
            }
        }

        let mut dirty = dirty?;
        let tail = next_annotations
            .get(self.first_changed_idx..)
            .unwrap_or(&[]);
        // Fixed-point propagation ensures upper pixel effects overlapping any dirty region
        // (including Blur halo neighborhoods) are fully recomputed without seams.
        loop {
            let mut expanded = dirty;
            for item in tail {
                match &item.kind {
                    AnnotationKind::Mosaic { region, .. } => {
                        let r = region.normalized();
                        if r.intersects_inclusive(expanded) {
                            expanded = expanded.union(r.expand(1.0));
                        }
                    }
                    AnnotationKind::GaussianBlur { region, radius } => {
                        let r = region.normalized();
                        let halo = ((*radius).clamp(1, 64) as f32) * 3.0 + 4.0;
                        if r.expand(halo).intersects_inclusive(expanded) {
                            expanded = expanded.union(r.expand(1.0));
                        }
                    }
                    _ => {}
                }
            }
            if expanded == dirty {
                break;
            }
            dirty = expanded;
        }

        dirty.to_clamped_pixel_rect(self.canvas_w, self.canvas_h)
    }

    fn sync_bgra_rect(&mut self, clip: PixelRect, cancel: &CancellationToken) -> Result<(), Error> {
        if let Some(bgra) = &mut self.bgra_single {
            swizzle_rgba_rect_to_bgra(&self.work_rgba, bgra, clip);
            return Ok(());
        }
        for (spec, tile_bgra) in &mut self.cached_tiles {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if tile_intersects(*spec, clip) {
                *tile_bgra = spec.extract_bgra(&self.work_rgba);
            }
        }
        Ok(())
    }

    fn build_payload(&self) -> Result<PreparedPayload, Error> {
        if let Some(bgra) = &self.bgra_single {
            return Ok(PreparedPayload::Single(bgra.clone()));
        }
        if self.tile_grid.needs_tiling() {
            return Ok(PreparedPayload::Tiled {
                tiles: self.cached_tiles.clone(),
            });
        }
        Ok(PreparedPayload::Single(color::clone_as_bgra(
            &self.work_rgba,
        )))
    }
}

fn resolve_transformed_base(
    base_rgba: Arc<RgbaImage>,
    cached_base: Option<(TransformState, Arc<RgbaImage>)>,
    transform: TransformState,
) -> Result<Arc<RgbaImage>, Error> {
    if transform.is_identity() {
        return Ok(base_rgba);
    }
    if let Some((cached_tf, cached_img)) = cached_base
        && cached_tf == transform
    {
        return Ok(cached_img);
    }
    Ok(Arc::new(apply_transform(&base_rgba, &transform.spec)?))
}

fn copy_rgba_rect(src: &RgbaImage, dst: &mut RgbaImage, clip: PixelRect) {
    let Some(rect) = clip.clamp_to(dst.width().min(src.width()), dst.height().min(src.height()))
    else {
        return;
    };
    let src_stride = (src.width() as usize).saturating_mul(4);
    let dst_stride = (dst.width() as usize).saturating_mul(4);
    let x_byte = (rect.x as usize).saturating_mul(4);
    let row_bytes = (rect.width as usize).saturating_mul(4);
    let src_raw = src.as_raw();
    let dst_raw: &mut [u8] = dst.as_mut();

    for row in rect.y as usize..rect.bottom() as usize {
        let s = row * src_stride + x_byte;
        let d = row * dst_stride + x_byte;
        dst_raw[d..d + row_bytes].copy_from_slice(&src_raw[s..s + row_bytes]);
    }
}

fn swizzle_rgba_rect_to_bgra(src: &RgbaImage, dst: &mut RgbaImage, clip: PixelRect) {
    let Some(rect) = clip.clamp_to(dst.width().min(src.width()), dst.height().min(src.height()))
    else {
        return;
    };
    let stride = (src.width() as usize).saturating_mul(4);
    let x_byte = (rect.x as usize).saturating_mul(4);
    let row_bytes = (rect.width as usize).saturating_mul(4);
    let src_raw = src.as_raw();
    let dst_raw: &mut [u8] = dst.as_mut();

    for row in rect.y as usize..rect.bottom() as usize {
        let start = row * stride + x_byte;
        let src_slice = &src_raw[start..start + row_bytes];
        let dst_slice = &mut dst_raw[start..start + row_bytes];
        for (s_px, d_px) in src_slice
            .as_chunks::<4>()
            .0
            .iter()
            .zip(dst_slice.as_chunks_mut::<4>().0.iter_mut())
        {
            *d_px = [s_px[2], s_px[1], s_px[0], s_px[3]];
        }
    }
}

const fn tile_intersects(spec: TileSpec, clip: PixelRect) -> bool {
    let tx1 = spec.x.saturating_add(spec.width);
    let ty1 = spec.y.saturating_add(spec.height);
    spec.x < clip.right() && tx1 > clip.x && spec.y < clip.bottom() && ty1 > clip.y
}
