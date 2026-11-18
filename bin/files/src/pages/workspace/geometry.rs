//! The file surface shares one geometry for rendering, navigation, and hit testing.

use gds::app::Gds;
use gds::appearance::tokens::{CONTENT_PADDING, DataListStyle};
use zgui::prelude::*;

use super::ui::Browser;

pub(super) const TABLE_INSET: f32 = CONTENT_PADDING;
pub(super) const GRID_PADDING: f32 = 8.0;
pub(super) const GRID_GAP: f32 = 8.0;
pub(super) const GRID_INSET: f32 = TABLE_INSET + GRID_PADDING;
pub(super) const SCROLLBAR_LANE: f32 = DataListStyle::SCROLLBAR_LANE;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RectF {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Grid {
    pub columns: usize,
    pub width: f32,
    pub row_height: f32,
    pub tile_height: f32,
    pub artwork_height: f32,
    pub name_height: f32,
    pub name_size: f32,
    pub line_height: f32,
    pub icon_size: f32,
}

#[derive(Clone, Copy)]
pub(super) struct GridMotion {
    pub target: Signal<Grid, LocalStorage>,
}

impl GridMotion {
    pub fn new(browser: &Browser) -> Self {
        let browser = browser.clone();
        let target = Signal::derive_local(move || browser.grid_geometry());
        Self { target }
    }
}

pub(super) fn virtual_row_height(height: f32, scale: f32) -> f32 {
    let scale = scale.max(1.0);
    (height * scale).round() / scale
}

impl Grid {
    pub fn new(width: f32, scale: f32) -> Self {
        let artwork_height = 100.0 * scale;
        let line_height = (20.0 * scale).ceil();
        let name_height = line_height * 2.0 + 2.0;
        let tile_height = GRID_PADDING * 2.0 + 2.0 + artwork_height + GRID_GAP + name_height;
        let available = (width - GRID_PADDING * 2.0 - SCROLLBAR_LANE).max(1.0);
        let columns = ((available + GRID_GAP) / (148.0 * scale + GRID_GAP))
            .floor()
            .max(1.0) as usize;
        Self {
            columns,
            width: (available - GRID_GAP * (columns - 1) as f32) / columns as f32,
            row_height: tile_height + GRID_PADDING * 2.0,
            tile_height,
            artwork_height,
            name_height,
            name_size: 14.0 * scale,
            line_height,
            icon_size: 48.0 * scale,
        }
    }

    pub fn tile(self, index: usize, scale: f32) -> RectF {
        RectF::new(
            GRID_INSET + (index % self.columns) as f32 * (self.width + GRID_GAP),
            GRID_PADDING
                + (index / self.columns) as f32 * virtual_row_height(self.row_height, scale),
            self.width,
            self.tile_height,
        )
    }
}

impl Browser {
    pub(super) fn compact(&self) -> bool {
        self.0.window_width.get() < 1080.0
    }

    pub(super) fn compact_untracked(&self) -> bool {
        self.0.window_width.get_untracked() < 1080.0
    }

    pub(super) fn sidebar_expanded(&self) -> bool {
        self.0.sidebar.get() && !self.compact()
    }

    pub(super) fn grid_geometry(&self) -> Grid {
        let scale = Gds::current().appearance().typography().scaled(1.0);
        Grid::new(self.0.content_width.get(), scale)
    }

    pub(super) fn grid_geometry_untracked(&self) -> Grid {
        let scale = Gds::current()
            .appearance()
            .typography()
            .scaled_untracked(1.0);
        Grid::new(self.0.content_width.get_untracked(), scale)
    }

    pub(super) fn table_viewport_width(&self) -> f32 {
        (self.0.content_width.get() - TABLE_INSET * 2.0).max(1.0)
    }

    pub(super) fn grid_columns_untracked(&self) -> usize {
        self.grid_geometry_untracked().columns
    }

    pub(super) fn row_height_untracked(&self) -> f32 {
        if self.read_untracked(|state| state.tab().mode == crate::entities::file::ViewMode::Grid) {
            self.grid_geometry_untracked().row_height
        } else {
            DataListStyle::ROW_HEIGHT
        }
    }
}
