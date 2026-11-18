//! Reactive controller and PTY lifecycle handle for a reusable terminal surface.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use tokio::sync::watch;
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, Owner, RenderEffect, RwSignal, Signal, UnsyncCallback};

use super::view::palette::Palette;
use super::{Event, Preferences};
use crate::app::Gds;
use crate::i18n;
use crate::service::task_manager::{Owner as TaskOwner, Scope};
use crate::service::terminal::{
    self as service, Error, Session, Shutdown, SpawnOptions, Status, input as terminal_input,
};

#[cfg(all(test, unix))]
mod tests;

const OUTPUT_COALESCE_INTERVAL: Duration = Duration::from_millis(16);

struct TerminalInner {
    scope: TaskOwner,
    service: service::Terminal,
    preferences: RwSignal<Preferences, LocalStorage>,
    status: RwSignal<Status, LocalStorage>,
    title: RwSignal<Option<String>, LocalStorage>,
    error: RwSignal<Option<String>, LocalStorage>,
    redraw: RwSignal<u64, LocalStorage>,
    focused: RwSignal<bool, LocalStorage>,
    focus_target: RefCell<Option<NodeRef>>,
    focus_requested: Cell<bool>,
    callbacks: RefCell<Vec<UnsyncCallback<Event>>>,
    observed: RefCell<(Status, Option<String>)>,
    shutdown_done: watch::Receiver<bool>,
    theme_effect: RefCell<Option<RenderEffect<()>>>,
    owner: Owner,
    #[cfg(test)]
    test_geometry: Rc<Cell<super::view::grid::GridGeometry>>,
    #[cfg(test)]
    test_cursor: Cell<(bool, bool, bool)>,
}

impl Drop for TerminalInner {
    fn drop(&mut self) {
        self.scope.cancel();
        *self.theme_effect.borrow_mut() = None;
        self.callbacks.borrow_mut().clear();
        self.owner.cleanup();
    }
}

/// Interactive terminal controller owning PTY startup, updates, theme sync, and shutdown.
#[derive(Clone)]
pub struct Terminal(Rc<TerminalInner>);

impl Terminal {
    /// Spawn a new terminal controller under `tasks` without tying its lifetime to a view.
    ///
    /// # Errors
    /// Returns an error if the terminal configuration cannot be prepared for launch.
    pub fn spawn(
        mut options: SpawnOptions,
        preferences: Preferences,
        tasks: &Scope,
    ) -> Result<Self, Error> {
        let preferences = preferences.normalized();
        if options.shell.is_none() {
            options.shell.clone_from(&preferences.shell);
        }
        if options.args.is_empty() && !preferences.shell_args.is_empty() {
            options.args.clone_from(&preferences.shell_args);
        }
        let default_options = SpawnOptions::default();
        if options.scrollback == default_options.scrollback {
            options.scrollback = preferences.scrollback_lines;
        }
        if options.palette.is_none() {
            options.palette = Some(Palette::fallback().query_colors());
        }

        let gds = Gds::try_current();
        let scope = tasks.named_child(format_args!("Terminal · {}", options.cwd.display()));
        let (service, launch) = service::Terminal::prepare((*scope).clone(), options);

        let cancel = scope.token();
        let cleanup = service.clone();
        let (finished, shutdown_done) = watch::channel(false);
        scope.spawn_named("Shut down terminal", async move {
            cancel.cancelled().await;
            cleanup.shutdown().await;
            finished.send_replace(true);
        });
        scope.spawn_blocking_named("Launch terminal", move || launch.run());

        let (ui_tx, mut ui_rx) = watch::channel(0_u64);
        let observing = service.clone();
        scope.spawn_cancellable_named("Observe terminal", async move {
            let notify_ui = || {
                ui_tx.send_modify(|revision| *revision = revision.wrapping_add(1));
            };
            let Some(session) = observing.ready().await else {
                notify_ui();
                return;
            };
            let mut updates = session.updates();
            let mut stopped = *updates.borrow_and_update();
            notify_ui();
            if stopped {
                return;
            }
            while updates.changed().await.is_ok() {
                tokio::time::sleep(OUTPUT_COALESCE_INTERVAL).await;
                stopped = *updates.borrow_and_update();
                notify_ui();
                if stopped {
                    break;
                }
            }
            notify_ui();
        });

        let owner = Owner::new();
        let inner = owner.with(|| {
            let initial_status = service.status();
            let initial_title = service.title().filter(|value| !value.is_empty());
            let preferences_signal = RwSignal::new_local(preferences);
            let status_signal = RwSignal::new_local(initial_status.clone());
            let title_signal = RwSignal::new_local(initial_title.clone());
            let error_signal = RwSignal::new_local(match &initial_status {
                Status::Failed(message) => Some(message.clone()),
                _ => None,
            });
            let redraw_signal = RwSignal::new_local(0_u64);
            let focused_signal = RwSignal::new_local(false);
            let callbacks: RefCell<Vec<UnsyncCallback<Event>>> = RefCell::new(Vec::new());
            let observed = RefCell::new((initial_status, initial_title));

            let theme_effect = gds.as_ref().map(|gds| {
                let prepared = gds.appearance().theme().prepared();
                let palette_service = service.clone();
                RenderEffect::new(move |_| {
                    prepared.with(|_| ());
                    palette_service.set_palette(Palette::current().query_colors());
                    redraw_signal.try_update(|revision| *revision = revision.wrapping_add(1));
                })
            });

            Rc::new(TerminalInner {
                scope,
                service,
                preferences: preferences_signal,
                status: status_signal,
                title: title_signal,
                error: error_signal,
                redraw: redraw_signal,
                focused: focused_signal,
                focus_target: RefCell::new(None),
                focus_requested: Cell::new(false),
                callbacks,
                observed,
                shutdown_done,
                theme_effect: RefCell::new(theme_effect),
                owner: owner.clone(),
                #[cfg(test)]
                test_geometry: Rc::new(Cell::new(super::view::grid::GridGeometry::default())),
                #[cfg(test)]
                test_cursor: Cell::new((false, false, true)),
            })
        });

        let weak = Rc::downgrade(&inner);
        owner.with(|| {
            zgui::reactive::spawn_local(async move {
                while ui_rx.changed().await.is_ok() {
                    let Some(strong) = weak.upgrade() else {
                        break;
                    };
                    sync_from_service(&strong);
                }
            });
        });

        Ok(Self(inner))
    }

    /// Replace active preferences (`normalized`); shell changes apply to future sessions only.
    pub fn apply_preferences(&self, prefs: Preferences) {
        let normalized = prefs.normalized();
        if self.0.preferences.get_untracked() != normalized {
            self.0.preferences.set(normalized);
        }
    }

    /// Reactive stream of normalized terminal preferences.
    #[must_use]
    pub fn preferences(&self) -> Signal<Preferences, LocalStorage> {
        self.0.preferences.into()
    }

    /// Reactive terminal window/process title reported via OSC title sequences.
    #[must_use]
    pub fn title(&self) -> Signal<Option<String>, LocalStorage> {
        self.0.title.into()
    }

    /// Reactive convenience flag indicating whether startup or shell execution is active.
    #[must_use]
    pub fn running(&self) -> Signal<bool, LocalStorage> {
        let status = self.0.status;
        Signal::derive_local(move || status.get().is_running())
    }

    /// Reactive lifecycle status of the terminal process.
    #[must_use]
    pub fn status(&self) -> Signal<Status, LocalStorage> {
        self.0.status.into()
    }

    /// Reactive I/O error banner text cleared on the next successful write.
    #[must_use]
    pub fn error(&self) -> Signal<Option<String>, LocalStorage> {
        self.0.error.into()
    }

    /// Working directory originally configured for this terminal.
    #[must_use]
    pub fn cwd(&self) -> &Path {
        self.0.service.cwd()
    }

    /// Current non-reactive check of whether the terminal is still alive.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.0.status.get_untracked().is_running()
    }

    /// Localized label describing the current terminal lifecycle state.
    #[must_use]
    pub fn status_label(&self) -> String {
        match self.0.status.get_untracked() {
            Status::Starting => i18n::text("gds-terminal-starting"),
            Status::Running => i18n::text("gds-terminal-running"),
            Status::Stopping => i18n::text("gds-terminal-stopping"),
            Status::Closed => i18n::text("gds-terminal-exited"),
            Status::Failed(error) => {
                format!("{}: {error}", i18n::text("gds-terminal-start-failed"))
            }
            Status::Exited { code, signal } => crate::t!(
                "gds-terminal-exit-status",
                code = code,
                signal = signal.unwrap_or_default()
            ),
        }
    }

    /// Write raw keyboard or escape sequence bytes to the running PTY session.
    pub fn write(&self, bytes: &[u8]) {
        let Some(session) = self.0.service.session() else {
            return;
        };
        match session.write(bytes) {
            Ok(()) => {
                if self.0.error.get_untracked().is_some() {
                    self.0.error.set(None);
                }
            }
            Err(error) => {
                let message = error.to_string();
                if self.0.error.get_untracked().as_deref() != Some(message.as_str()) {
                    self.0.error.set(Some(message));
                }
            }
        }
    }

    /// Resize the PTY character grid directly.
    pub fn resize(&self, cols: u16, rows: u16) {
        if let Some(session) = self.0.service.session() {
            let _ = session.resize_with_pixels(cols, rows, 0, 0);
        }
    }

    /// Move keyboard focus to the mounted `TerminalView`, or defer until it mounts.
    pub fn focus(&self) {
        self.0.focus_requested.set(!self.0.focused.get_untracked());
        if let Some(node) = *self.0.focus_target.borrow() {
            node.focus();
        }
    }

    /// Register a callback invoked for [`Event`] transitions on the UI thread.
    pub fn on_event(&self, cb: UnsyncCallback<Event>) {
        self.0.callbacks.borrow_mut().push(cb);
    }

    /// Change the active shell working directory asynchronously.
    ///
    /// # Errors
    /// Reports an unavailable session, invalid path, or unverified shell input.
    pub fn change_directory(
        &self,
        cwd: PathBuf,
    ) -> impl Future<Output = anyhow::Result<()>> + Send + 'static + use<> {
        let session = (!self.0.service.is_closing())
            .then(|| self.0.service.session())
            .flatten();
        async move {
            let session = session.ok_or(Error::ShellBusy)?;
            session.change_directory(&cwd).await?;
            Ok(())
        }
    }

    /// Paste quoted file paths without submitting the command line.
    pub fn paste_paths(&self, paths: &[PathBuf]) {
        match terminal_input::path_text(paths) {
            Ok(text) => self.paste_text(&text),
            Err(error) => self.set_error(Some(error.to_string())),
        }
    }

    /// Encode text respecting bracketed paste mode and write it to the PTY.
    pub fn paste_text(&self, text: &str) {
        let Some(session) = self.0.service.session() else {
            return;
        };
        let bytes = session.with_term(|term| terminal_input::paste_bytes(text, *term.mode()));
        self.write(&bytes);
    }

    /// Terminate the PTY process cleanly and await service cleanup completion.
    pub fn shutdown(&self) -> Shutdown {
        self.0.scope.cancel();
        drop(self.0.service.shutdown());
        sync_from_service(&self.0);
        let mut done = self.0.shutdown_done.clone();
        Box::pin(async move {
            let _ = done.wait_for(|finished| *finished).await;
        })
    }

    pub(super) fn session(&self) -> Option<Session> {
        self.0.service.session()
    }

    pub(super) fn redraw_revision(&self) -> Signal<u64, LocalStorage> {
        self.0.redraw.into()
    }

    pub(super) fn request_redraw(&self) {
        self.0
            .redraw
            .try_update(|revision| *revision = revision.wrapping_add(1));
    }

    pub(super) fn focused_signal(&self) -> RwSignal<bool, LocalStorage> {
        self.0.focused
    }

    pub(super) fn register_focus_target(&self, node: Option<NodeRef>) {
        *self.0.focus_target.borrow_mut() = node;
        if self.0.focus_requested.replace(false)
            && let Some(target) = node
        {
            target.focus();
        }
    }

    pub(super) fn set_error(&self, message: Option<String>) {
        if self
            .0
            .error
            .try_get_untracked()
            .is_some_and(|cur| cur != message)
        {
            let _ = self.0.error.try_set(message);
        }
    }

    #[cfg(test)]
    pub(super) fn ready(&self) -> impl Future<Output = Option<Session>> + Send + 'static + use<> {
        self.0.service.ready()
    }

    #[cfg(test)]
    pub(super) fn set_test_cursor(&self, focused: bool, blink_active: bool, cursor_on: bool) {
        self.0.test_cursor.set((focused, blink_active, cursor_on));
    }

    #[cfg(test)]
    pub(super) fn test_geometry_cell(&self) -> Rc<Cell<super::view::grid::GridGeometry>> {
        Rc::clone(&self.0.test_geometry)
    }

    #[cfg(test)]
    pub(super) fn test_geometry(&self) -> super::view::grid::GridGeometry {
        self.0.test_geometry.get()
    }

    #[cfg(test)]
    pub(super) fn test_cursor_state(&self) -> (bool, bool, bool) {
        self.0.test_cursor.get()
    }
}

fn sync_from_service(inner: &TerminalInner) {
    let next_status = inner.service.status();
    let next_title = inner.service.title().filter(|value| !value.is_empty());

    let prev = inner.observed.borrow().clone();
    let state_changed = prev.0 != next_status || prev.1 != next_title;
    let title_changed = prev.1 != next_title;
    let exited_code = if prev.0 != next_status {
        match &next_status {
            Status::Exited { code, .. } => Some(i32::try_from(*code).ok()),
            Status::Closed | Status::Failed(_) => Some(None),
            Status::Starting | Status::Running | Status::Stopping => None,
        }
    } else {
        None
    };

    *inner.observed.borrow_mut() = (next_status.clone(), next_title.clone());

    if inner
        .status
        .try_get_untracked()
        .is_some_and(|cur| cur != next_status)
    {
        let error_text = match &next_status {
            Status::Failed(message) => Some(message.clone()),
            _ => None,
        };
        let _ = inner.status.try_set(next_status);
        if let Some(error_text) = error_text {
            let _ = inner.error.try_set(Some(error_text));
        }
    }
    if inner
        .title
        .try_get_untracked()
        .is_some_and(|cur| cur != next_title)
    {
        let _ = inner.title.try_set(next_title.clone());
    }
    let _ = inner
        .redraw
        .try_update(|revision| *revision = revision.wrapping_add(1));

    if title_changed && let Some(title) = next_title {
        emit_event(inner, &Event::TitleChanged(title));
    }
    if let Some(code) = exited_code {
        emit_event(inner, &Event::Closed(code));
    }
    if state_changed {
        emit_event(inner, &Event::Changed);
    }
}

fn emit_event(inner: &TerminalInner, event: &Event) {
    let listeners: Vec<_> = inner.callbacks.borrow().iter().copied().collect();
    for cb in listeners {
        cb.run(event.clone());
    }
}
