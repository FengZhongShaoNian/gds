//! Asynchronous ordered writer and application-wide preference snapshot.

use std::{
    ops::Deref,
    sync::{Arc, Mutex},
};

use gds::{
    app::{AppExt as _, Context as GdsContext, Gds},
    service::task_manager::Scope,
};
use gpui_kit::{App, AppContext, Context, Entity, EventEmitter, Global, Task};
use tokio::sync::mpsc;

use super::{Preferences, api, presentation};

pub struct SaveFailed(pub String);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Storage(#[from] api::Error),
    #[error(transparent)]
    Presentation(#[from] presentation::Error),
    #[error("GDS Images is shutting down; preferences can no longer be modified")]
    Closed,
}

type Queue = Arc<Mutex<Option<mpsc::UnboundedSender<Preferences>>>>;

struct Installed(Entity<Service>);
impl Global for Installed {}

/// Single-read startup snapshot carried from blocking Tokio preparation into GPUI bootstrap.
pub struct StartupBundle {
    preferences: Preferences,
    store: api::Store,
    notice: Option<String>,
}

impl StartupBundle {
    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    pub fn preferences_mut(&mut self) -> &mut Preferences {
        &mut self.preferences
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }
}

/// Reads and normalizes startup preferences in memory without touching disk.
///
/// # Errors
/// Returns configuration directory or I/O read failures.
pub fn prepare_startup() -> Result<StartupBundle, Error> {
    let mut loaded = api::read_startup()?;
    presentation::normalize(&mut loaded.preferences);
    Ok(StartupBundle {
        preferences: loaded.preferences,
        store: loaded.store,
        notice: loaded.notice,
    })
}

pub struct Service {
    current: Preferences,
    queue: Queue,
    _updates: Task<()>,
}

impl EventEmitter<SaveFailed> for Service {}

impl Service {
    pub fn current(&self) -> &Preferences {
        &self.current
    }

    fn update(&mut self, mut next: Preferences, cx: &mut Context<Self>) -> Result<(), Error> {
        presentation::normalize(&mut next);
        let prepared = presentation::Prepared::new(&next, &self.current, cx)?;
        let queue = self.queue.lock().expect("preferences queue lock");
        let sender = queue.as_ref().ok_or(Error::Closed)?;
        if cx.gds().tasks().is_shutting_down() || sender.is_closed() {
            return Err(Error::Closed);
        }
        sender.send(next.clone()).map_err(|_| Error::Closed)?;
        prepared.install(&next, cx);
        if self.current != next {
            self.current = next;
            cx.notify();
        }
        Ok(())
    }

    fn new(
        preferences: Preferences,
        mut store: api::Store,
        tasks: &Scope,
        cx: &mut Context<Self>,
    ) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<Preferences>();
        let queue = Arc::new(Mutex::new(Some(sender)));
        let closing = queue.clone();
        let cancel = tasks.token();
        tasks.spawn_named("Close images preferences writer", async move {
            cancel.cancelled().await;
            closing.lock().expect("preferences queue lock").take();
        });

        let (results, mut saved) = mpsc::unbounded_channel();
        let writer = tasks.spawn_blocking_named("Save images preferences", move || {
            while let Some(mut next) = receiver.blocking_recv() {
                while let Ok(newer) = receiver.try_recv() {
                    next = newer;
                }
                let result = store.save(&next);
                if let Err(error) = &result {
                    tracing::error!(%error, "cannot save GDS Images preferences");
                }
                let _ = results.send(result);
            }
        });

        let updates = cx.spawn(async move |this, cx| {
            while let Some(result) = saved.recv().await {
                if let Err(error) = result {
                    let _ = this.update(cx, |_, cx| cx.emit(SaveFailed(error.to_string())));
                }
            }
            if let Err(error) = writer.await {
                tracing::error!(%error, "images preferences writer stopped");
                let _ = this.update(cx, |_, cx| cx.emit(SaveFailed(error.to_string())));
            }
        });

        Self {
            current: preferences,
            queue,
            _updates: updates,
        }
    }
}

/// Scoped accessor exposing preferences state and mutations on [`gds::app::Gds`].
#[derive(Clone, Copy)]
pub struct Accessor<C> {
    cx: C,
}

pub trait PreferencesExt {
    type Context<'a>: GdsContext
    where
        Self: 'a;

    fn preferences(&mut self) -> Accessor<Self::Context<'_>>;
}

impl<C: GdsContext> PreferencesExt for Gds<C> {
    type Context<'a>
        = C::Reborrowed<'a>
    where
        C: 'a;

    fn preferences(&mut self) -> Accessor<C::Reborrowed<'_>> {
        Accessor {
            cx: self.reborrow().into_inner(),
        }
    }
}

impl<C: Deref<Target = App>> Accessor<C> {
    pub fn current(&self) -> Preferences {
        self.service().read(&self.cx).current.clone()
    }

    pub fn try_current(&self) -> Option<Preferences> {
        self.cx
            .try_global::<Installed>()
            .map(|installed| installed.0.read(&self.cx).current.clone())
    }

    pub fn service(&self) -> Entity<Service> {
        self.cx.global::<Installed>().0.clone()
    }
}

impl Accessor<&mut App> {
    /// Installs the preferences service from a pre-loaded [`StartupBundle`].
    pub fn initialize(&mut self, startup: StartupBundle, tasks: &Scope) -> Entity<Service> {
        assert!(
            !self.cx.has_global::<Installed>(),
            "preferences already initialized"
        );
        let service = self
            .cx
            .new(|cx| Service::new(startup.preferences, startup.store, tasks, cx));
        self.cx.set_global(Installed(service.clone()));
        service
    }

    /// Initializes test-isolated preferences backed by a temporary file path.
    #[cfg(all(test, feature = "gui-tests"))]
    pub fn init_test(
        &mut self,
        mut preferences: Preferences,
        path: std::path::PathBuf,
        tasks: &Scope,
    ) -> Entity<Service> {
        presentation::normalize(&mut preferences);
        self.initialize(
            StartupBundle {
                preferences,
                store: api::test_store(path),
                notice: None,
            },
            tasks,
        )
    }

    /// Commits a modified preference snapshot and enqueues it for background disk persistence.
    ///
    /// # Errors
    /// Returns presentation errors or [`Error::Closed`] during application shutdown.
    pub fn update(&mut self, preferences: Preferences) -> Result<(), Error> {
        let service = self.service();
        service.update(self.cx, |service, cx| service.update(preferences, cx))
    }

    /// Edits the current preferences snapshot atomically.
    ///
    /// # Errors
    /// Returns presentation errors or [`Error::Closed`] during application shutdown.
    pub fn edit(&mut self, edit: impl FnOnce(&mut Preferences)) -> Result<(), Error> {
        let mut preferences = self.current();
        edit(&mut preferences);
        self.update(preferences)
    }
}
