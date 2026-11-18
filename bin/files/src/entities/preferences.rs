//! Persisted application preferences and their editor model.
mod api;
mod model;
mod presentation;
mod service;

pub use api::load;
#[allow(unused_imports)]
pub use model::{FilesPreferences, Preferences, PreferencesEditor};
pub(crate) use service::*;
