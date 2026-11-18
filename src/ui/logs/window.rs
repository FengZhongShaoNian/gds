//! Secondary diagnostic log-viewer window lifecycle and state registry.

use std::cell::RefCell;
use std::sync::OnceLock;

use zgui::runtime::{WindowHandle, try_use_window, try_use_windows};
use zgui::view;
#[cfg(test)]
use zgui::vocab::{Key, Modifiers};

use super::LogViewerProps;
use crate::i18n;
use crate::service::logs::Store;
use crate::ui::frame::FrameProps;
use crate::ui::window::{TitleBarProps, workspace_options};

pub(crate) const INITIAL_WIDTH: f32 = 960.0;
pub(crate) const INITIAL_HEIGHT: f32 = 640.0;
pub(crate) const MIN_WIDTH: f32 = 520.0;
pub(crate) const MIN_HEIGHT: f32 = 340.0;

static STATE: OnceLock<State> = OnceLock::new();

thread_local! {
    static LOG_WINDOW: RefCell<Option<WindowHandle>> = const { RefCell::new(None) };
}

struct State {
    store: Store,
    options: Options,
}

/// Marker type representing the open-logs command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct OpenLogs;

/// Startup configuration enabling the opt-in diagnostic viewer.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub app_id: Option<String>,
    pub app_name: String,
    pub ignored_contexts: &'static [&'static str],
}

/// Initialize global diagnostic log collection with the given host [`Options`].
pub fn init(options: Options) {
    let _ = STATE.set(State {
        store: Store::new(),
        options,
    });
}

/// Return whether diagnostic log collection has been enabled via [`init`].
#[must_use]
pub fn is_enabled() -> bool {
    STATE.get().is_some()
}

/// Return the shared diagnostic [`Store`] when enabled.
#[must_use]
pub fn store() -> Option<Store> {
    STATE.get().map(|state| state.store.clone())
}

/// Return `true` when `key` + `modifiers` match the global `Ctrl+Shift+L` / `Cmd+Shift+L` toggle.
#[cfg(test)]
#[must_use]
pub(crate) fn is_open_shortcut(key: &Key, modifiers: Modifiers) -> bool {
    let chord = Modifiers::PRIMARY | Modifiers::SHIFT;
    modifiers == chord && matches!(key, Key::Character(ch) if ch.eq_ignore_ascii_case("l"))
}

/// Open the singleton secondary diagnostic viewer window, or focus it if already open.
pub fn open() {
    let Some(state) = STATE.get() else {
        return;
    };

    if let Some(existing) = LOG_WINDOW.with(|slot| slot.borrow().clone())
        && existing.is_open()
    {
        existing.focus();
        return;
    }

    let Some(windows) = try_use_windows() else {
        tracing::warn!("logs window requested outside an active zgui windows context");
        return;
    };

    let options = workspace_options(i18n::text("logs-title"), state.options.app_id.as_deref())
        .with_size(INITIAL_WIDTH, INITIAL_HEIGHT)
        .with_min_size(MIN_WIDTH, MIN_HEIGHT);
    let handle = windows.open(options, || {
        view! {
            Frame {
                TitleBar(title = i18n::text("logs-title"))
                LogViewer()
            }
        }
    });
    LOG_WINDOW.with(|slot| {
        *slot.borrow_mut() = Some(handle);
    });
}

pub(crate) fn close_active() {
    if let Some(window) = try_use_window() {
        window.close();
        return;
    }
    LOG_WINDOW.with(|slot| {
        if let Some(handle) = slot.borrow_mut().take()
            && handle.is_open()
        {
            handle.close();
        }
    });
}
