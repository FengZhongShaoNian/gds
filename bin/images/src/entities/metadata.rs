//! Editable metadata draft and Linux `user.xdg.tags` state for the active image.

mod api;
mod model;

pub use api::{load_tags, save_tags};
pub use model::MetadataState;
