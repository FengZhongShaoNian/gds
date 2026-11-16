//! File opening prompt and zoom/fit navigation helpers.

use std::path::PathBuf;

use gpui_kit::{App, PathPromptOptions, Task};

use crate::shared::{
    geometry::{FitMode, Point2, Size2, ViewportTransform},
    i18n::text,
};

const ZOOM_STEP: f32 = 1.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomPreset {
    ZoomIn,
    ZoomOut,
    Fit,
    Actual,
}

/// Opens the native system file picker filtered for opening a single image.
pub fn open_file_dialog(cx: &mut App) -> Task<Option<PathBuf>> {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(text("action-open", cx)),
    });
    cx.spawn(async move |_| {
        paths
            .await
            .ok()
            .and_then(Result::ok)
            .flatten()
            .and_then(|list| list.into_iter().next())
    })
}

/// Applies a zoom/fit preset to `tf` centered around the current viewport.
pub fn apply_zoom_preset(
    preset: ZoomPreset,
    tf: &mut ViewportTransform,
    viewport: Size2,
    image: Size2,
) {
    let center = Point2::new(viewport.width * 0.5, viewport.height * 0.5);
    match preset {
        ZoomPreset::ZoomIn => tf.zoom_around_cursor(ZOOM_STEP, center),
        ZoomPreset::ZoomOut => tf.zoom_around_cursor(1.0 / ZOOM_STEP, center),
        ZoomPreset::Fit => {
            *tf = ViewportTransform::compute_fit(FitMode::FitToWindow, viewport, image)
        }
        ZoomPreset::Actual => {
            *tf = ViewportTransform::compute_fit(FitMode::ActualPixels, viewport, image);
        }
    }
}
