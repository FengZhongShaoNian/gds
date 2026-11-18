#[cfg(test)]
mod tests;

use std::rc::Rc;

use zgui::reactive::{LocalStorage, RwSignal, Signal, UnsyncCallback, prelude::*};

use super::{Event, Preferences};
use crate::service::{
    task_manager::Scope,
    terminal::{Error, SpawnOptions, Status},
};

struct TerminalInner {
    title: RwSignal<Option<String>, LocalStorage>,
    running: RwSignal<bool, LocalStorage>,
    status: RwSignal<Status, LocalStorage>,
    prefs: RwSignal<Preferences, LocalStorage>,
}

/// Clonable PTY controller whose lifetime outlives view mount/unmount cycles.
#[derive(Clone)]
pub struct Terminal {
    inner: Rc<TerminalInner>,
}

impl Terminal {
    /// Spawn a terminal PTY session under `tasks`.
    ///
    /// # Errors
    /// Returns an error if PTY creation or command spawning fails.
    pub fn spawn(
        options: SpawnOptions,
        preferences: Preferences,
        tasks: &Scope,
    ) -> Result<Self, Error> {
        let _ = (options, tasks);
        Ok(Self {
            inner: Rc::new(TerminalInner {
                title: RwSignal::new_local(None),
                running: RwSignal::new_local(true),
                status: RwSignal::new_local(Status::Running),
                prefs: RwSignal::new_local(preferences),
            }),
        })
    }

    pub fn apply_preferences(&self, prefs: Preferences) {
        self.inner.prefs.set(prefs);
    }

    #[must_use]
    pub fn preferences(&self) -> Signal<Preferences, LocalStorage> {
        self.inner.prefs.into()
    }

    #[must_use]
    pub fn title(&self) -> Signal<Option<String>, LocalStorage> {
        self.inner.title.into()
    }

    #[must_use]
    pub fn running(&self) -> Signal<bool, LocalStorage> {
        self.inner.running.into()
    }

    #[must_use]
    pub fn status(&self) -> Signal<Status, LocalStorage> {
        self.inner.status.into()
    }

    pub fn write(&self, bytes: &[u8]) {
        let _ = bytes;
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = (cols, rows);
    }

    pub fn focus(&self) {}

    pub fn on_event(&self, cb: UnsyncCallback<Event>) {
        let _ = cb;
    }
}
