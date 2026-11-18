use std::cell::RefCell;

use zgui::prelude::*;

use super::LogViewerProps;
use crate::app::Gds;
use crate::service::logs::Store;
use crate::ui::window::workspace_options;

/// Action token triggering the diagnostics log window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct OpenLogs;

/// Application identity and filter policy for the shared log window.
#[derive(Clone)]
pub struct Options {
    pub app_id: Option<String>,
    pub app_name: String,
    pub ignored_contexts: &'static [&'static str],
}

impl Default for Options {
    fn default() -> Self {
        Self {
            app_id: None,
            app_name: "GDS".to_owned(),
            ignored_contexts: &[],
        }
    }
}

thread_local! {
    static REGISTERED: RefCell<Option<Options>> = const { RefCell::new(None) };
}

/// Register log viewer options; reads the collector from `Gds::current().ui().logs().store()`.
pub fn init(options: Options) {
    REGISTERED.with(|slot| *slot.borrow_mut() = Some(options));
}

#[must_use]
pub fn is_enabled() -> bool {
    store().is_some()
}

#[must_use]
pub fn store() -> Option<Store> {
    Gds::try_current().and_then(|gds| gds.ui().logs().store())
}

/// Open the diagnostics log window when a log collector is active.
pub fn open() {
    if !is_enabled() {
        return;
    }
    let Some(windows) = zgui::prelude::try_use_windows() else {
        return;
    };
    let (title, app_id) = REGISTERED.with(|slot| {
        slot.borrow().as_ref().map_or_else(
            || ("Logs".to_owned(), None),
            |opt| (format!("{} Logs", opt.app_name), opt.app_id.clone()),
        )
    });
    let options = workspace_options(title, app_id.as_deref())
        .with_size(1_000., 720.)
        .with_min_size(780., 480.);
    windows.open(options, || view! { LogViewer() });
}
