//! File presentation shared by the window's directory tabs.

use super::*;
use gpui_kit::component::table::TableState;
use std::{cell::Cell, rc::Rc};

pub(in crate::pages::workspace) struct State {
    pub table: Entity<TableState<table::FileTable>>,
    pub table_leased: Rc<Cell<bool>>,
    pub grid_selection: grid::SelectionCache,
    pub columns: Entity<columns::Editor>,
    pub marquee: marquee::State,
    pub preview: Entity<preview::View>,
    pub inspections: Entity<ui::file_inspection::Cache>,
}

impl State {
    pub(in crate::pages::workspace) fn new(
        preferences: &configuration::Preferences,
        window: &mut Window,
        cx: &mut Context<Browser>,
    ) -> Self {
        let inspections =
            cx.new(|_| ui::file_inspection::Cache::new(preferences.files.preview.cache_options()));
        cx.observe(&inspections, |_, _, cx| cx.notify()).detach();
        let table_leased = Rc::new(Cell::new(false));
        Self {
            table: table::FileTable::make_state(
                cx.entity().downgrade(),
                Rc::clone(&table_leased),
                window,
                cx,
            ),
            table_leased,
            grid_selection: grid::SelectionCache::default(),
            columns: cx.new(|_| columns::Editor::new()),
            marquee: marquee::State::default(),
            preview: cx.new(|cx| preview::View::new(inspections.clone(), cx)),
            inspections,
        }
    }
}
