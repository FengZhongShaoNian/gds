//! Settings widget view emitting updated preferences without owning workspace navigation.

mod ui;

#[cfg(all(test, feature = "gui-tests"))]
pub use ui::pages::Page;
pub use ui::{Event, View};
