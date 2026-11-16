//! Composited Straight RGBA8 output and BGRA8 GPUI RenderImage / TileGrid cache.

#[cfg(test)]
mod tests;

use std::sync::Arc;

use gds::service::image::{
    Error, color,
    ops::{apply_transform, composite_ops},
    tiles::{DEFAULT_MAX_TILE_EDGE, TileGrid, TileSpec},
};
use gpui_kit::{App, RenderImage};
use image::{Frame, RgbaImage};
use tokio_util::sync::CancellationToken;

use super::{annotation::AnnotationItem, transform::TransformState};

/// Off-thread output holding composited RGBA8 (for save/copy) and straight BGRA8 frames.
#[derive(Clone)]
pub struct PreparedTextures {
    pub composited_rgba: Arc<RgbaImage>,
    pub payload: PreparedPayload,
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
    pub composited: Option<Arc<RgbaImage>>,
    pub composited_revision: u64,
    pub output: Option<RenderOutput>,
    pub generation: u64,
}

impl RenderCache {
    pub fn composited_at(&self, revision: u64) -> Option<&Arc<RgbaImage>> {
        self.composited
            .as_ref()
            .filter(|_| self.composited_revision == revision)
    }

    pub fn release_textures(&mut self, cx: &mut App) {
        let Some(old) = self.output.take() else {
            return;
        };
        match old {
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

    pub fn install(
        &mut self,
        prepared: PreparedTextures,
        generation: u64,
        revision: u64,
        cx: &mut App,
    ) {
        self.release_textures(cx);
        self.composited = Some(prepared.composited_rgba);
        self.composited_revision = revision;
        self.output = Some(upload_payload(prepared.payload));
        self.generation = generation;
    }

    /// Replaces only the displayed GPU textures (used by SVG zoom re-rasterization)
    /// without altering `composited` or `composited_revision`.
    pub fn install_display_output(
        &mut self,
        payload: PreparedPayload,
        generation: u64,
        cx: &mut App,
    ) {
        self.release_textures(cx);
        self.output = Some(upload_payload(payload));
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
/// Reuses `base_rgba` directly via `Arc::clone` when no transform or annotation edits exist.
///
/// # Errors
/// Returns [`Error::Cancelled`], [`Error::UnsupportedOperation`], [`Error::Svg`], or
/// [`Error::TooLarge`] if cancelled or rendering fails.
pub fn build_render_textures(
    base_rgba: Arc<RgbaImage>,
    transform: TransformState,
    annotations: &[AnnotationItem],
    cancel: &CancellationToken,
) -> Result<PreparedTextures, Error> {
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }

    let composited_rgba = if transform.is_identity() && annotations.is_empty() {
        Arc::clone(&base_rgba)
    } else {
        let mut composited = if transform.is_identity() {
            (*base_rgba).clone()
        } else {
            apply_transform(&base_rgba, &transform.spec)?
        };
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if !annotations.is_empty() {
            let ops: Vec<_> = annotations
                .iter()
                .map(AnnotationItem::to_raster_op)
                .collect();
            composite_ops(&mut composited, &ops)?;
        }
        Arc::new(composited)
    };

    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }

    let payload = encode_display_payload(&composited_rgba, cancel)?;
    Ok(PreparedTextures {
        composited_rgba,
        payload,
    })
}

/// Converts a zoomed SVG rasterization into Straight-BGRA8 GPUI display textures.
///
/// # Errors
/// Returns [`Error::Cancelled`] if superseded before upload preparation completes.
pub fn build_svg_zoom_textures(
    rgba: Arc<RgbaImage>,
    cancel: &CancellationToken,
) -> Result<PreparedPayload, Error> {
    encode_display_payload(&rgba, cancel)
}

fn encode_display_payload(
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
