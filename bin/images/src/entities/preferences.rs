//! Persisted user preferences for GDS Images.

mod api;
mod model;
mod presentation;
mod service;

pub use model::{
    DEFAULT_INSPECTOR_WIDTH, DEFAULT_JPEG_QUALITY, MAX_INSPECTOR_WIDTH, MIN_INSPECTOR_WIDTH,
    Preferences,
};
pub use service::{
    Accessor as PreferencesAccessor, Error, PreferencesExt, SaveFailed, Service, StartupBundle,
    prepare_startup,
};
