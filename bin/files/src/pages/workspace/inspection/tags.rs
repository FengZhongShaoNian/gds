#![allow(dead_code)]
//! Chip creation, removal and draft typing share the original editing history.

mod history;

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

use zgui::prelude::*;
use zgui::view;

use super::super::ui::{Browser, WeakBrowser};
use history::History;

#[derive(Clone)]
pub(in crate::pages::workspace) struct TagEditor {
    browser: WeakBrowser,
    path: PathBuf,
    history: Rc<RefCell<History>>,
    saving: RwSignal<bool, LocalStorage>,
}

impl TagEditor {
    pub(super) fn new(browser: &Browser, path: PathBuf) -> Self {
        Self {
            browser: browser.downgrade(),
            path,
            history: Rc::new(RefCell::new(History::default())),
            saving: RwSignal::new_local(false),
        }
    }

    pub(in crate::pages::workspace) fn is_saving(&self) -> bool {
        self.saving.get_untracked()
    }

    pub(super) fn available(&self) -> bool {
        self.browser
            .upgrade()
            .is_some_and(|browser| browser.0.preferences.snapshot().tags.enabled)
    }

    pub(super) fn focus(&self) {}

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn save(&self, complete: impl FnOnce() + 'static) {
        complete();
    }

    pub(super) fn fields(&self) -> AnyView {
        AnyView::new(view! { column() })
    }
}
