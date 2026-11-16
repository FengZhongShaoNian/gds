//! Document state, non-destructive spatial transforms, annotations, and GPUI texture cache.

mod annotation;
mod cache;
mod model;
mod transform;

pub use annotation::{
    AnnotationId, AnnotationItem, AnnotationKind, RgbaColor, SpatialTransformOp, measure_text_rect,
};
#[cfg(test)]
pub use cache::CompleteComposite;
pub use cache::{
    GesturePrepareInput, GestureSession, PreparedPayload, PreparedTextures, RenderCache,
    RenderOutput, SceneTag, TransientFrame, build_render_textures, build_svg_zoom_textures,
    encode_display_payload,
};
pub use model::DocumentState;
pub use transform::TransformState;
