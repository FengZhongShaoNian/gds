//! Application entry point wiring CLI/stdin startup, supervisor lifecycle, and main window.

mod cli;
mod config;
mod ui;
mod windows;

use std::process::ExitCode;

use anyhow::Context as _;
use clap::Parser as _;
use gds::{
    app::AppExt as _,
    service::{logs::Store, task_manager::Scope},
};
use gpui_kit::QuitMode;
use tokio::sync::oneshot;

use self::{
    cli::Arguments,
    config::{AppConfig, StartupOutcome, resolve_startup},
    ui::Initializer,
};
use crate::{entities::preferences, shared::ui::assets::Assets};

/// Runs `gds-images`, exiting silently with `0` if pre-GUI stdin receives 0 bytes.
pub fn run() -> ExitCode {
    match Bootstrap::load().and_then(|maybe| match maybe {
        Some(bootstrap) => bootstrap.run(),
        None => Ok(()),
    }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gds-images: {error:#}");
            ExitCode::FAILURE
        }
    }
}

struct Bootstrap {
    config: AppConfig,
    logs: Option<Store>,
    supervisor: gds::service::task_manager::Supervisor,
}

impl Bootstrap {
    fn load() -> anyhow::Result<Option<Self>> {
        let args = Arguments::parse();
        let StartupOutcome::Launch(config) = resolve_startup(&args)? else {
            return Ok(None);
        };
        let logs = gds::service::logs::init_tracing(
            "warn,gds=debug,gds_images=debug",
            config.enable_logs,
        )?;
        let supervisor = gds::service::task_manager::Supervisor::new(config.enable_logs)
            .context("cannot start the application task supervisor")?;
        Ok(Some(Self {
            config,
            logs,
            supervisor,
        }))
    }

    fn bind_lifecycle(tasks: Scope, cx: &mut gpui_kit::App) {
        cx.gds_mut().tasks().init(tasks);
        #[cfg(unix)]
        cx.gds_mut()
            .tasks()
            .signals(|cx| cx.gds_mut().tasks().request_quit());
        cx.on_window_closed(|cx, _| cx.gds_mut().tasks().quit_when_idle())
            .detach();
    }

    async fn prepare_io(tasks: Scope) -> anyhow::Result<PreparedIo> {
        let startup_prefs = tasks
            .spawn_blocking_named("Load images preferences", preferences::prepare_startup)
            .await??;
        startup_prefs.preferences().appearance.prime_session().await;
        Ok(PreparedIo { startup_prefs })
    }

    fn run(self) -> anyhow::Result<()> {
        let Self {
            config,
            logs,
            supervisor,
        } = self;
        // `supervisor` owns the Tokio runtime and must stay alive until the GPUI event loop exits.
        let tasks = supervisor.scope();
        let (completed, mut startup) = oneshot::channel();

        gpui_kit::application()
            .with_quit_mode(QuitMode::Explicit)
            .with_assets(Assets)
            .run(move |cx| {
                cx.spawn(async move |cx| {
                    cx.update(|cx| Self::bind_lifecycle(tasks.clone(), cx));

                    let io = tasks
                        .spawn_cancellable_named(
                            "Start Images services",
                            Self::prepare_io(tasks.clone()),
                        )
                        .await;

                    let result = cx.update(move |cx| {
                        if cx.gds().tasks().is_shutting_down() {
                            return Ok(());
                        }
                        let prepared = io??;
                        prepared.mount(&tasks, config, logs, cx)
                    });
                    if let Err(error) = &result {
                        tracing::error!(%error, "Cannot start Images");
                        cx.update(|cx| cx.gds_mut().tasks().request_quit());
                    }
                    let _ = completed.send(result);
                })
                .detach();
            });

        match startup.try_recv() {
            Ok(Ok(()))
            | Err(oneshot::error::TryRecvError::Empty | oneshot::error::TryRecvError::Closed) => {
                Ok(())
            }
            Ok(Err(error)) => Err(error),
        }
    }
}

struct PreparedIo {
    startup_prefs: preferences::StartupBundle,
}

impl PreparedIo {
    fn mount(
        self,
        tasks: &Scope,
        config: AppConfig,
        logs: Option<Store>,
        cx: &mut gpui_kit::App,
    ) -> anyhow::Result<()> {
        let mut notices = Vec::new();
        Initializer::new(cx).run(tasks, &config, self.startup_prefs, logs, &mut notices)?;
        windows::open_main_window(tasks.clone(), config, cx)?;
        for notice in notices {
            gds::ui::notifications::error(notice, cx);
        }
        // Transition to ready only after the initial window exists so idle-quit checks never
        // observe a 0-window window during startup.
        cx.gds_mut().tasks().ready(false);
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            build = env!("GDS_BUILD_TAG"),
            "Images ready"
        );
        Ok(())
    }
}
