//! GPUI runtime services, theme/typography initialization, preferences, and keybindings.

use gds::{
    app::AppExt as _,
    appearance::{theme, typography},
    i18n as gds_i18n,
    service::task_manager::Scope,
    ui::notifications::{Level, Report},
};
use gpui_kit::App;

use super::config::AppConfig;
use crate::{
    entities::preferences::{self, PreferencesExt as _},
    shared::{i18n, ui::actions},
};

pub(super) struct Initializer<'a> {
    cx: &'a mut App,
}

impl<'a> Initializer<'a> {
    pub fn new(cx: &'a mut App) -> Self {
        Self { cx }
    }

    /// Initializes shared GDS services, theme, typography, i18n, preferences, and keyboard actions.
    ///
    /// # Errors
    /// Returns errors if Fluent translation loading or built-in theme fallback fails.
    pub fn run(
        &mut self,
        tasks: &Scope,
        config: &AppConfig,
        mut startup_prefs: preferences::StartupBundle,
        logs: Option<gds::service::logs::Store>,
        notices: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        self.cx.gds_mut().init_ui(logs);

        let app_title = if config.is_capture {
            "GDS Images (Capture)"
        } else {
            "GDS Images"
        };
        self.cx.gds_mut().ui().logs().init(gds::ui::logs::Options {
            app_id: Some(config.app_id.into()),
            app_name: app_title.into(),
            ..Default::default()
        });
        self.cx.set_app_identity(config.app_id, app_title);

        i18n::install(self.cx)?;
        if let Some(error) = startup_prefs.notice() {
            notices.push(
                gds_i18n::t!(
                    @self.cx,
                    "settings-load-failed",
                    error = error.to_owned()
                )
                .into(),
            );
        }

        let animations_enabled = startup_prefs
            .preferences()
            .animations_enabled
            .unwrap_or(true);
        let font = startup_prefs.preferences().font.clone();

        self.cx
            .gds_mut()
            .appearance()
            .motion()
            .set_enabled(animations_enabled);
        self.cx.gds_mut().appearance().motion().follow_system();

        self.apply_theme(&mut startup_prefs.preferences_mut().appearance, notices)?;
        self.apply_typography(&font, notices);
        self.cx
            .gds_mut()
            .appearance()
            .theme()
            .init_hot_reload(startup_prefs.preferences().appearance.clone());

        self.install_preferences(startup_prefs, tasks);
        self.cx.gds_mut().ui().register_actions();
        actions::bind_defaults(self.cx);
        Ok(())
    }

    fn apply_theme(
        &mut self,
        appearance: &mut theme::Appearance,
        notices: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        let Err(error) = self.cx.gds_mut().appearance().theme().apply(appearance) else {
            return Ok(());
        };
        let unavailable_system = appearance.source == theme::Source::System;
        if unavailable_system {
            tracing::warn!(%error, "system theme unavailable; using built-in theme");
        } else {
            notices.push(error.to_string());
        }
        let fallback = appearance.builtin_fallback();
        if let Err(error) = self.cx.gds_mut().appearance().theme().apply(&fallback) {
            tracing::error!("Cannot initialize the built-in theme: {error}");
            return Err(error.into());
        }
        if unavailable_system {
            *appearance = fallback;
        }
        Ok(())
    }

    fn apply_typography(&mut self, font: &typography::Preferences, notices: &mut Vec<String>) {
        match self.cx.gds_mut().appearance().typography().apply(font) {
            Ok(applied) => {
                if let Some(unavailable) = applied.unavailable {
                    tracing::error!(
                        "Font family {unavailable:?} is not installed; using the system font."
                    );
                }
            }
            Err(error) => {
                notices.push(error.to_string());
                if let Err(fallback) = self
                    .cx
                    .gds_mut()
                    .appearance()
                    .typography()
                    .apply(&typography::Preferences::default())
                {
                    tracing::error!("Cannot initialize the default font: {fallback}");
                }
            }
        }
    }

    fn install_preferences(&mut self, startup: preferences::StartupBundle, tasks: &Scope) {
        let prefs_entity = self.cx.gds_mut().preferences().initialize(startup, tasks);
        self.cx
            .subscribe(&prefs_entity, |_, event: &preferences::SaveFailed, cx| {
                Report::new(
                    Level::Error,
                    gds_i18n::t!(@cx, "settings-save-failed", error = event.0.clone()),
                )
                .replacing::<preferences::SaveFailed>()
                .show(cx);
            })
            .detach();
    }
}
