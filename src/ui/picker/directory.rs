//! Source-independent directory chooser built over reactive signals.

mod commands;
mod model;
mod table;
#[cfg(test)]
mod tests;
mod view;

use std::{future::Future, pin::Pin, rc::Rc};

pub use model::{Entry, Location, Page, Request};
pub use view::DirectoryPickerView;

/// Asynchronous directory page loader; dropping the future cancels in-flight work.
pub type Loader = Rc<dyn Fn(Request) -> Pin<Box<dyn Future<Output = Result<Page, String>>>>>;

/// Reactive state controller backing [`DirectoryPickerView`].
#[derive(Clone)]
pub struct DirectoryPicker {
    #[allow(dead_code)] // Seed skeleton for Wave 2 ui-overlays-data.
    pub(crate) loader: Loader,
}

impl DirectoryPicker {
    #[must_use]
    pub fn new(loader: Loader) -> Self {
        Self { loader }
    }
}
