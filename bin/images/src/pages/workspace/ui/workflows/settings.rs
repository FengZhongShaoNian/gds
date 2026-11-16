//! Lazy settings widget lifecycle, live preference application, and persistence errors.

use gds::{i18n::t, ui::notifications::Level};
use gpui_kit::{Context, Entity, Window};

use super::*;
use crate::{
    entities::preferences::{
        Error as PreferencesError, Preferences, PreferencesAccessor, PreferencesExt as _,
        Service as PreferencesService,
    },
    widgets::settings,
};

impl WorkspacePage {
    pub(in crate::pages::workspace::ui) fn open_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush_editing_text_before_external(window, cx);
        self.cancel_pointer_gesture_before_external(cx);
        self.set_app_menu_open(false, cx);
        self.settings.open = true;
        let sidebar_state = self.sidebar_state();
        let sidebar_pres = self.chrome.sidebar_presentation;
        let view = self.ensure_settings_view(window, cx);
        view.update(cx, |settings, cx| {
            settings.set_sidebar_presentation(sidebar_state, sidebar_pres, cx);
            settings.refresh_themes(window, cx);
            settings.focus_active_page(window, cx);
        });
        cx.notify();
    }

    pub(in crate::pages::workspace::ui) fn close_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.open {
            return;
        }
        self.settings.open = false;
        self.focus.canvas.focus(window, cx);
        cx.notify();
    }

    fn ensure_settings_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<settings::View> {
        if let Some(view) = &self.settings.view {
            return view.clone();
        }
        let accessor: PreferencesAccessor<_> = cx.gds().preferences();
        let _service: Entity<PreferencesService> = accessor.service();
        let prefs = accessor.try_current().unwrap_or_else(|| accessor.current());
        let sidebar_state = self.sidebar_state();
        let sidebar_pres = self.chrome.sidebar_presentation;
        let tasks = self.tasks.clone();
        let view =
            cx.new(|cx| settings::View::new(tasks, prefs, sidebar_state, sidebar_pres, window, cx));
        self.settings.subscription = Some(cx.subscribe_in(
            &view,
            window,
            |this, _, event: &settings::Event, window, cx| match event {
                settings::Event::Apply(prefs) => {
                    this.apply_preferences(prefs.clone(), window, cx);
                }
            },
        ));
        self.settings.view = Some(view.clone());
        view
    }

    fn apply_preferences(
        &mut self,
        prefs: Preferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_chrome_from_preferences(&prefs);
        self.settings.applying_from_view = true;
        let result = cx.gds_mut().preferences().update(prefs);
        self.settings.applying_from_view = false;
        if let Err(error) = result {
            self.report_preferences_error(&error, cx);
        }
        let _ = window;
        cx.notify();
    }

    fn report_preferences_error(&self, error: &PreferencesError, cx: &mut Context<Self>) {
        self.report_settings_save_error(&error.to_string(), cx);
    }

    pub(in crate::pages::workspace::ui) fn sync_preferences(
        &mut self,
        prefs: &Preferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_chrome_from_preferences(prefs);
        if !self.settings.applying_from_view
            && let Some(view) = &self.settings.view
        {
            let next = prefs.clone();
            view.update(cx, |settings, cx| {
                settings.set_preferences(next, window, cx)
            });
        }
        cx.notify();
    }

    fn sync_chrome_from_preferences(&mut self, prefs: &Preferences) {
        self.chrome.filmstrip_expanded = prefs.filmstrip_open;
        self.chrome.inspector_visible = prefs.inspector_open;
        self.chrome.inspector_width = prefs.effective_inspector_width();
    }

    pub(in crate::pages::workspace::ui) fn report_settings_save_error(
        &self,
        error: &str,
        cx: &mut Context<Self>,
    ) {
        show_report::<SettingsNotice>(
            Level::Error,
            t!(@cx, "settings-save-failed", error = error),
            cx,
        );
    }
}
