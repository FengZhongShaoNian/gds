//! Native application services shared across windows on one UI thread.
use std::{cell::RefCell, rc::Rc};
use zgui::reactive::{Owner, on_cleanup_local, provide_local_context, use_local_context};

mod task_manager;
pub use task_manager::Tasks;
#[cfg(feature = "appearance")]
pub mod appearance;
#[cfg(feature = "appearance")]
pub use appearance::Appearance;
#[cfg(feature = "i18n")]
pub mod i18n;
#[cfg(feature = "i18n")]
pub use i18n::I18n;
#[cfg(feature = "ui")]
pub mod ui;
#[cfg(feature = "ui")]
pub use ui::Ui;

struct Services {
    owner: Owner,
    _tokio: Option<zgui::tokio::Installed>,
    tasks: Tasks,
    #[cfg(feature = "appearance")]
    appearance: Appearance,
    #[cfg(feature = "i18n")]
    i18n: I18n,
    #[cfg(feature = "ui")]
    ui: Ui,
}

thread_local! {
    static CURRENT: RefCell<Option<Gds>> = const { RefCell::new(None) };
}

/// Shared application services context. Clones point to the same UI-thread registry.
#[derive(Clone)]
pub struct Gds(Rc<Services>);

impl Gds {
    /// Install application services inside the active `zgui` reactive root before opening windows.
    ///
    /// Binds the Tokio supervisor's executor into `zgui-tokio` (when present) and exposes `Gds`
    /// both as a reactive local context and on the current UI thread.
    pub fn install(scope: crate::service::task_manager::Scope) -> Self {
        #[cfg(feature = "ui")]
        return Self::install_inner(scope, None);
        #[cfg(not(feature = "ui"))]
        return Self::install_inner(scope);
    }

    /// Install application services together with an optional UI log collector.
    #[cfg(feature = "ui")]
    pub fn install_with_logs(
        scope: crate::service::task_manager::Scope,
        logs: Option<crate::service::logs::Store>,
    ) -> Self {
        Self::install_inner(scope, logs)
    }

    fn install_inner(
        scope: crate::service::task_manager::Scope,
        #[cfg(feature = "ui")] logs: Option<crate::service::logs::Store>,
    ) -> Self {
        let owner = Owner::current().expect("install GDS inside the application reactive root");
        let tokio_guard = scope.runtime_handle().and_then(|handle| {
            match zgui::tokio::install_handle(handle.clone()) {
                Ok(installed) => Some(installed),
                Err(error) => {
                    tracing::debug!(
                        %error,
                        "background executor already active; entering supervisor runtime on UI thread"
                    );
                    zgui::tokio::enter_here(handle.clone());
                    None
                }
            }
        });
        let tasks_scope = scope.clone();
        let services = Self(Rc::new(Services {
            owner: owner.clone(),
            _tokio: tokio_guard,
            tasks: Tasks::new(scope, owner),
            #[cfg(feature = "appearance")]
            appearance: Appearance::new(),
            #[cfg(feature = "i18n")]
            i18n: I18n::new(),
            #[cfg(feature = "ui")]
            ui: Ui::new(logs),
        }));
        CURRENT.with(|slot| {
            assert!(
                slot.borrow().is_none(),
                "application services must be installed once per UI thread"
            );
            *slot.borrow_mut() = Some(services.clone());
        });
        provide_local_context(services.clone());
        on_cleanup_local(move || {
            tasks_scope.cancel();
            CURRENT.with(|slot| {
                slot.borrow_mut().take();
            });
        });
        services
    }

    /// Look up the installed [`Gds`] context on the current UI thread, if active.
    pub fn try_current() -> Option<Self> {
        if zgui::reactive::is_ui_thread()
            && let Some(services) = use_local_context::<Self>()
        {
            return Some(services);
        }
        CURRENT.with(|slot| slot.borrow().clone())
    }

    /// Return the installed [`Gds`] context for the current UI thread.
    pub fn current() -> Self {
        Self::try_current().expect("install GDS services before accessing Gds::current()")
    }

    /// Run `f` under the application-wide reactive owner rather than a single window owner.
    pub fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        self.0.owner.with(f)
    }

    pub fn tasks(&self) -> &Tasks {
        &self.0.tasks
    }

    #[cfg(feature = "appearance")]
    pub fn appearance(&self) -> &Appearance {
        &self.0.appearance
    }

    #[cfg(feature = "i18n")]
    pub fn i18n(&self) -> &I18n {
        &self.0.i18n
    }

    #[cfg(feature = "ui")]
    pub fn ui(&self) -> &Ui {
        &self.0.ui
    }
}
