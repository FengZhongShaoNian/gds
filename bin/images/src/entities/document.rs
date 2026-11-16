//! Document state, non-destructive spatial transforms, annotations, and GPUI texture cache.

mod annotation;
mod cache;
mod model;
mod transform;

pub use annotation::{AnnotationId, AnnotationItem, AnnotationKind, RgbaColor};
pub use cache::{
    PreparedPayload, PreparedTextures, RenderCache, RenderOutput, build_render_textures,
    build_svg_zoom_textures,
};
pub use model::DocumentState;
pub use transform::TransformState;
