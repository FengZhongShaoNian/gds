#![allow(dead_code)]
//! A window-owned workspace. Services finish on Tokio; `zgui` owners receive results.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::OsString,
    future::Future,
    path::PathBuf,
    rc::{Rc, Weak},
};

use gds::{
    app::Gds,
    appearance::tokens::COLLAPSED_SIDEBAR_WIDTH,
    i18n,
    service::task_manager::{Owner as TaskOwner, Scope},
};
use zgui::prelude::*;
use zgui::reactive::RenderEffect;

use super::{
    model::{AddressPresentation, Contents, Operation, Pages, Pane, Prompt, State, Tab},
    name_form::{NameForm, NamePurpose},
};
use crate::{
    entities::{
        file::columns::Column,
        location::{
            Location,
            places::{Error as PlacesError, Places},
            volumes,
        },
        preferences,
    },
    features::pick_files,
    shared::lib::clipboard::Clipboard,
};

pub(crate) struct LaunchOptions {
    pub places: Result<Places, PlacesError>,
    pub start: PathBuf,
    pub home: PathBuf,
    pub picker: Option<pick_files::Options>,
}

#[derive(Clone)]
pub(crate) struct Browser(pub(super) Rc<Workspace>);

pub(crate) struct WeakBrowser(Weak<Workspace>);

impl Clone for WeakBrowser {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl WeakBrowser {
    pub(crate) fn upgrade(&self) -> Option<Browser> {
        self.0
            .upgrade()
            .map(Browser)
            .filter(|browser| browser.0.alive.get())
    }
}

pub(super) struct Workspace {
    pub alive: Cell<bool>,
    pub state: RefCell<State>,
    pub revision: RwSignal<u64, LocalStorage>,
    pub selection: RwSignal<u64, LocalStorage>,
    pub column_widths: RwSignal<HashMap<Column, f32>, LocalStorage>,
    pub pages: RwSignal<Pages, LocalStorage>,
    pub window_width: RwSignal<f32, LocalStorage>,
    pub content_width: RwSignal<f32, LocalStorage>,
    pub address_presentation: RwSignal<AddressPresentation, LocalStorage>,
    pub sidebar_tabs_height: RwSignal<Option<f32>, LocalStorage>,
    pub settings: crate::widgets::settings::View,
    pub sidebar: RwSignal<bool, LocalStorage>,
    pub details: RwSignal<bool, LocalStorage>,
    pub terminal: RwSignal<bool, LocalStorage>,
    pub notice: RwSignal<Option<String>, LocalStorage>,
    pub prompt: RwSignal<Prompt, LocalStorage>,
    pub address: RwSignal<String, LocalStorage>,
    pub filter: RwSignal<String, LocalStorage>,
    pub save_name: RwSignal<String, LocalStorage>,
    pub list: NodeRef,
    pub content_focus: NodeRef,
    pub root_focus: NodeRef,
    pub list_focused: Cell<bool>,
    pub list_focus_visible: RwSignal<bool, LocalStorage>,
    pub settings_node: NodeRef,
    pub address_node: NodeRef,
    pub filter_node: NodeRef,
    pub tabs_node: NodeRef,
    pub locations_node: NodeRef,
    pub volumes_node: NodeRef,
    pub sidebar_region: RwSignal<Option<super::keyboard::SidebarRegion>, LocalStorage>,
    pub sidebar_cursor: RwSignal<Option<super::keyboard::SidebarRow>, LocalStorage>,
    pub sidebar_reveal_volumes: RwSignal<bool, LocalStorage>,
    pub tasks: TaskOwner,
    pub owner: Owner,
    pub window: Option<WindowHandle>,
    pub volumes: volumes::Service,
    pub preferences: preferences::Service,
    pub new_window: Rc<dyn Fn(PathBuf)>,
    pub inspections: RwSignal<gds::ui::file_inspection::Cache, LocalStorage>,
    pub terminals: super::terminals::Dock,
    pub connector: crate::features::connect_server::Connector,
    pub menus: super::menus::Menus,
    pub picker_result: Option<Rc<dyn Fn(Vec<PathBuf>)>>,
    pub pending_focus: RefCell<Option<zgui::view::time::FrameHandle>>,
}

impl Browser {
    pub(crate) fn new(
        launch: LaunchOptions,
        volumes: volumes::Service,
        new_window: Rc<dyn Fn(PathBuf)>,
    ) -> Self {
        Self::new_with_output(launch, volumes, new_window, None)
    }

    pub(super) fn new_with_picker_result(
        launch: LaunchOptions,
        volumes: volumes::Service,
        new_window: Rc<dyn Fn(PathBuf)>,
        result: Rc<dyn Fn(Vec<PathBuf>)>,
    ) -> Self {
        Self::new_with_output(launch, volumes, new_window, Some(result))
    }

    fn new_with_output(
        launch: LaunchOptions,
        volumes: volumes::Service,
        new_window: Rc<dyn Fn(PathBuf)>,
        picker_result: Option<Rc<dyn Fn(Vec<PathBuf>)>>,
    ) -> Self {
        let LaunchOptions {
            places,
            start,
            home,
            picker,
        } = launch;
        let (places, notice) = match places {
            Ok(places) => (places, None),
            Err(error) => (Places::default(), Some(error.to_string())),
        };
        let location = Location::from(start.as_path());
        let mut tabs = if picker.is_none() {
            places
                .pinned
                .iter()
                .enumerate()
                .map(|(index, pinned)| {
                    Tab::new(index as u64 + 1, pinned.clone(), &places, true).restored()
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let active = tabs
            .iter()
            .position(|tab| tab.navigation.location() == &location)
            .unwrap_or_else(|| {
                tabs.push(Tab::new(
                    tabs.len() as u64 + 1,
                    location.clone(),
                    &places,
                    false,
                ));
                tabs.len() - 1
            });
        let next_tab = tabs.len() as u64 + 1;
        let save_name = picker
            .as_ref()
            .and_then(|options| options.portal.as_ref())
            .and_then(|request| match &request.mode {
                gds::service::desktop::protocol::chooser::Mode::Save { name } => {
                    Some(name.to_string_lossy().into_owned())
                }
                _ => None,
            })
            .unwrap_or_default();
        let owner = Owner::current().expect("Files workspace is created inside its window owner");
        let tasks = Gds::current().tasks().scope().named_child("Files window");
        let connector = crate::features::connect_server::Connector::new((*tasks).clone());
        let preference_service = preferences::Service::current();
        let inspections = RwSignal::new_local(gds::ui::file_inspection::Cache::new(
            preference_service.snapshot().files.preview.cache_options(),
        ));
        let terminals = super::terminals::Dock::new();
        let sidebar_tabs_height = preference_service
            .snapshot()
            .files
            .sidebar
            .tabs_height
            .filter(|height| height.is_finite() && *height > 0.0);
        let notice = RwSignal::new_local(notice);
        let apply = preference_service.clone();
        let settings =
            crate::widgets::settings::View::new(preference_service.state(), move |preferences| {
                if let Err(error) = apply.update(preferences) {
                    notice.set(Some(error.to_string()));
                }
            });
        let initial_width = 960.0;
        let initial_widths = Self::column_widths_map(&places, &location);
        let window = try_use_window();
        let browser = Self(Rc::new(Workspace {
            alive: Cell::new(true),
            details: RwSignal::new_local(places.details_visible(location.clone())),
            state: RefCell::new(State {
                tabs,
                active,
                next_tab,
                home,
                places,
                history: Default::default(),
                picker: picker.map(pick_files::Session::new),
                operation: Operation::Idle,
                saving_places: false,
                favorites_reload: None,
            }),
            revision: RwSignal::new_local(0),
            selection: RwSignal::new_local(0),
            column_widths: RwSignal::new_local(initial_widths),
            pages: RwSignal::new_local(Pages::default()),
            window_width: RwSignal::new_local(initial_width),
            content_width: RwSignal::new_local(initial_width - COLLAPSED_SIDEBAR_WIDTH),
            address_presentation: RwSignal::new_local(AddressPresentation::Breadcrumbs),
            sidebar_tabs_height: RwSignal::new_local(sidebar_tabs_height),
            settings,
            sidebar: RwSignal::new_local(true),
            terminal: RwSignal::new_local(false),
            notice,
            prompt: RwSignal::new_local(Prompt::Closed),
            address: RwSignal::new_local(location.display_address()),
            filter: RwSignal::new_local(String::new()),
            save_name: RwSignal::new_local(save_name),
            list: NodeRef::new(),
            content_focus: NodeRef::new(),
            root_focus: NodeRef::new(),
            list_focused: Cell::new(false),
            list_focus_visible: RwSignal::new_local(false),
            settings_node: NodeRef::new(),
            address_node: NodeRef::new(),
            filter_node: NodeRef::new(),
            tabs_node: NodeRef::new(),
            locations_node: NodeRef::new(),
            volumes_node: NodeRef::new(),
            sidebar_region: RwSignal::new_local(None),
            sidebar_cursor: RwSignal::new_local(None),
            sidebar_reveal_volumes: RwSignal::new_local(false),
            tasks,
            connector,
            inspections,
            terminals,
            menus: super::menus::Menus::new(),
            picker_result,
            pending_focus: RefCell::new(None),
            owner,
            window: window.clone(),
            volumes,
            preferences: preference_service,
            new_window,
        }));
        browser
            .0
            .state
            .borrow()
            .places
            .drain_on_close(&Gds::current().tasks().scope(), browser.0.tasks.token());
        let weak = browser.downgrade();
        on_cleanup_local(move || {
            if let Some(browser) = weak.0.upgrade() {
                browser.alive.set(false);
                *browser.pending_focus.borrow_mut() = None;
                browser.state.borrow().operation.cancel();
                browser.tasks.cancel();
            }
        });
        if window.is_some() {
            let close = browser.downgrade();
            let guard = on_close_request(move || {
                if let Some(browser) = close.upgrade() {
                    browser.cancel_picker();
                }
                CloseResponse::Close
            });
            on_cleanup_local(move || drop(guard));
        }
        if let Some(win) = window {
            let focused = win.focused();
            let focus_effect = RenderEffect::new(move |_| {
                if focused.get() {
                    Clipboard::current().refresh();
                }
            });
            on_cleanup_local(move || drop(focus_effect));
        }
        browser.observe_services();
        browser.observe_terminals();
        browser.initialize_picker();
        let ids = browser.read(|state| state.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>());
        for id in ids {
            browser.refresh_tab(id);
        }
        browser.focus_content_after_mount();
        browser
    }

    pub(super) fn focus_content_after_mount(&self) {
        if self.pane() == Pane::Files
            && matches!(
                self.0.address_presentation.get_untracked(),
                AddressPresentation::Breadcrumbs
            )
            && self.0.content_focus.is_bound()
        {
            self.focus_content();
            return;
        }
        let weak = self.downgrade();
        let handle = self.0.owner.with(|| {
            request_frame(move |_| {
                if let Some(browser) = weak.upgrade()
                    && browser.pane() == Pane::Files
                    && matches!(
                        browser.0.address_presentation.get_untracked(),
                        AddressPresentation::Breadcrumbs
                    )
                {
                    browser.focus_content();
                }
            })
        });
        *self.0.pending_focus.borrow_mut() = Some(handle);
    }

    pub(super) fn focus_content(&self) {
        if self.0.content_focus.is_bound() {
            self.0.content_focus.focus();
        } else {
            self.0.root_focus.focus();
        }
    }

    pub(super) fn pane(&self) -> Pane {
        self.0.pages.with(Pages::current)
    }

    pub(super) fn show_files(&self) {
        self.0.pages.update(|pages| pages.open(Pane::Files));
        self.focus_content_after_mount();
    }

    pub(super) fn back_page(&self) {
        self.0.pages.update(Pages::back);
        self.focus_page_after_mount();
    }

    pub(super) fn open_page(&self, pane: Pane) {
        self.finish_address_edit();
        self.0.pages.update(|pages| pages.open(pane));
        self.focus_page_after_mount();
    }

    fn focus_page_after_mount(&self) {
        if self.pane() == Pane::Files {
            self.focus_content_after_mount();
        } else {
            let node = self.0.settings_node;
            let handle = self.0.owner.with(|| request_frame(move |_| node.focus()));
            *self.0.pending_focus.borrow_mut() = Some(handle);
        }
    }

    pub(super) fn toggle_settings(&self) {
        if self.pane() == Pane::Settings {
            self.back_page();
        } else {
            self.show_settings();
        }
    }

    pub(super) fn show_settings(&self) {
        self.open_page(Pane::Settings);
    }

    pub(crate) fn downgrade(&self) -> WeakBrowser {
        WeakBrowser(Rc::downgrade(&self.0))
    }

    pub(super) fn track(&self) {
        let _ = self.0.revision.get();
        let _ = self.0.selection.get();
    }

    pub(super) fn changed(&self) {
        self.0
            .revision
            .update(|revision| *revision = revision.wrapping_add(1));
    }

    pub(super) fn read<R>(&self, read: impl FnOnce(&State) -> R) -> R {
        self.track();
        read(&self.0.state.borrow())
    }

    pub(super) fn update_selection<R>(&self, edit: impl FnOnce(&mut State) -> R) -> R {
        let result = edit(&mut self.0.state.borrow_mut());
        self.0
            .selection
            .update(|revision| *revision = revision.wrapping_add(1));
        result
    }

    pub(super) fn update<R>(&self, edit: impl FnOnce(&mut State) -> R) -> R {
        let value = edit(&mut self.0.state.borrow_mut());
        self.changed();
        value
    }

    pub(crate) fn report_notice(&self, notice: impl Into<String>) {
        self.0.notice.set(Some(notice.into()));
    }

    pub(super) fn report_success(&self, message: impl Into<String>) {
        gds::ui::feedback::notifications::success(message);
    }

    pub(super) fn scope(&self) -> Scope {
        (*self.0.tasks).clone()
    }

    pub(super) fn work<T: Send + 'static>(
        &self,
        name: &'static str,
        work: impl Future<Output = Result<T, String>> + Send + 'static,
        finished: impl FnOnce(&Browser, Result<T, String>) + 'static,
    ) {
        let browser = self.downgrade();
        let task = self.0.tasks.spawn_named(name, work);
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let result = task
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(std::convert::identity);
                if let Some(browser) = browser.upgrade() {
                    finished(&browser, result);
                }
            }));
        });
    }

    pub(super) fn blocking<T: Send + 'static>(
        &self,
        name: &'static str,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
        finished: impl FnOnce(&Browser, Result<T, String>) + 'static,
    ) {
        let browser = self.downgrade();
        let task = self.0.tasks.spawn_blocking_named(name, work);
        self.0.owner.with(|| {
            drop(spawn_local(async move {
                let result = task
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(std::convert::identity);
                if let Some(browser) = browser.upgrade() {
                    finished(&browser, result);
                }
            }));
        });
    }

    pub(super) fn save_places(&self) {
        let save = {
            let mut state = self.0.state.borrow_mut();
            if state.saving_places {
                return;
            }
            let Some(save) = state.places.take_save() else {
                return;
            };
            state.saving_places = true;
            save
        };
        self.work(
            "Save Files places",
            async move { save.run().await.map_err(|error| error.to_string()) },
            |browser, result| {
                browser.update(|state| {
                    state.saving_places = false;
                    if let Ok(snapshot) = result.as_ref() {
                        state.places.accept(snapshot.clone());
                    }
                });
                match result {
                    Ok(_) => browser.save_places(),
                    Err(error) => browser.report_notice(error),
                }
            },
        );
    }

    pub(super) fn edit_preferences(&self, edit: impl FnOnce(&mut preferences::Preferences)) {
        if let Err(error) = self.0.preferences.edit(edit) {
            self.report_notice(error.to_string());
        }
    }

    pub(super) fn confirm(&self, title: String, message: String, accept: impl Fn() + 'static) {
        self.confirm_labelled(title, message, i18n::text("dialog-confirm"), accept);
    }

    pub(super) fn confirm_labelled(
        &self,
        title: String,
        message: String,
        primary: String,
        accept: impl Fn() + 'static,
    ) {
        self.0.prompt.set(Prompt::Confirm {
            title,
            message,
            primary,
            accept: Rc::new(accept),
        });
    }

    pub(super) fn prompt_name(
        &self,
        title: String,
        initial: String,
        submit: impl Fn(String) + 'static,
    ) {
        self.name_form(
            NameForm::new(NamePurpose::Generic, title, initial.into()),
            move |name| {
                submit(
                    name.into_string()
                        .expect("generic name comes from a UTF-8 text field"),
                );
            },
        );
    }

    pub(super) fn name_form(&self, form: NameForm, submit: impl Fn(OsString) + 'static) {
        self.0.prompt.set(Prompt::Name {
            form,
            submit: Rc::new(submit),
            cancel: None,
        });
    }

    pub(super) fn name_form_with_cancel(
        &self,
        form: NameForm,
        submit: impl Fn(OsString) + 'static,
        cancel: impl Fn() + 'static,
    ) {
        self.0.prompt.set(Prompt::Name {
            form,
            submit: Rc::new(submit),
            cancel: Some(Rc::new(cancel)),
        });
    }

    pub(super) fn cancel_prompt(&self) {
        let prompt = self.0.prompt.get_untracked();
        self.0.prompt.set(Prompt::Closed);
        if let Prompt::Name {
            cancel: Some(cancel),
            ..
        } = prompt
        {
            cancel();
        }
    }
}

impl Browser {
    pub(super) fn column_widths_map(places: &Places, location: &Location) -> HashMap<Column, f32> {
        places
            .column_widths(location.clone())
            .into_iter()
            .filter_map(|(id, width)| Column::from_id(&id).map(|column| (column, width)))
            .collect()
    }

    fn observe_services(&self) {
        let previous = RefCell::new(self.0.preferences.snapshot());
        let browser = self.clone();
        let pref_fx = RenderEffect::new(move |_| {
            let preferences = browser.0.preferences.snapshot();
            let prior = previous.replace(preferences.clone());
            browser.apply_terminal_preferences(&preferences);
            if prior.files.preview != preferences.files.preview {
                browser
                    .0
                    .inspections
                    .set(gds::ui::file_inspection::Cache::new(
                        preferences.files.preview.cache_options(),
                    ));
            }
            if prior.files.show_hidden != preferences.files.show_hidden
                || prior.files.watching != preferences.files.watching
                || prior.files.streaming != preferences.files.streaming
            {
                let ids = browser
                    .0
                    .state
                    .borrow()
                    .tabs
                    .iter()
                    .filter(|tab| {
                        matches!(tab.contents, Contents::Directory | Contents::Remote { .. })
                    })
                    .map(|tab| tab.id)
                    .collect::<Vec<_>>();
                for id in ids {
                    browser.refresh_tab(id);
                }
            } else if prior.files.enrichment != preferences.files.enrichment
                || prior.files.probe_budget != preferences.files.probe_budget
            {
                let ids = browser
                    .0
                    .state
                    .borrow()
                    .tabs
                    .iter()
                    .map(|tab| tab.id)
                    .collect::<Vec<_>>();
                for id in ids {
                    browser.enrich_directory(id);
                }
            }
            if prior.tags != preferences.tags {
                let ids = browser
                    .0
                    .state
                    .borrow()
                    .tabs
                    .iter()
                    .filter(|tab| matches!(tab.navigation.location(), Location::Tags(_)))
                    .map(|tab| tab.id)
                    .collect::<Vec<_>>();
                for id in ids {
                    browser.refresh_tab(id);
                }
            }
        });
        on_cleanup_local(move || drop(pref_fx));

        let browser = self.clone();
        let volume_fx = RenderEffect::new(move |_| {
            let _ = browser.0.volumes.snapshot();
            let (collections, restored) = {
                let state = browser.0.state.borrow();
                let collections = state
                    .tabs
                    .iter()
                    .filter(|tab| matches!(tab.contents, Contents::Volumes))
                    .map(|tab| tab.id)
                    .collect::<Vec<_>>();
                let tab = state.tab();
                let navigation = &tab.navigation;
                let location = navigation.location();
                let restored = matches!(location, Location::Volume { .. })
                    && navigation.path().is_none()
                    && navigation.pending_path().is_none()
                    && navigation
                        .pending_location()
                        .is_none_or(|pending| pending == location)
                    && browser.resolve_path(location).is_some();
                (collections, restored.then_some(tab.id))
            };
            for id in collections {
                browser.reproject(id);
            }
            if let Some(id) = restored {
                browser.refresh_tab(id);
            }
        });
        on_cleanup_local(move || drop(volume_fx));

        let browser = self.clone();
        let error_fx = RenderEffect::new(move |_| {
            if let Some(error) = browser.0.preferences.error().get() {
                browser.report_notice(error);
            }
        });
        on_cleanup_local(move || drop(error_fx));
    }
}
