//! Settings navigation categories matching the GDS suite structure.

mod appearance;
mod export;
mod viewer;

use gpui_kit::{App, Entity, IntoElement, SharedString, Window, assets::IconName};

use super::View;
use crate::shared::i18n::text;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Page {
    #[default]
    Appearance,
    Viewer,
    Export,
}

impl Page {
    pub const ALL: [Self; 3] = [Self::Appearance, Self::Viewer, Self::Export];

    pub const fn index(self) -> usize {
        match self {
            Self::Appearance => 0,
            Self::Viewer => 1,
            Self::Export => 2,
        }
    }

    pub const fn label_id(self) -> &'static str {
        match self {
            Self::Appearance => "settings-appearance",
            Self::Viewer => "settings-viewer",
            Self::Export => "settings-export",
        }
    }

    pub const fn icon(self) -> IconName {
        match self {
            Self::Appearance => IconName::Palette,
            Self::Viewer => IconName::Image,
            Self::Export => IconName::ShieldCheck,
        }
    }

    pub fn label(self, cx: &App) -> SharedString {
        text(self.label_id(), cx)
    }

    pub fn step(self, delta: isize) -> Self {
        let count = Self::ALL.len() as isize;
        let next = (self.index() as isize + delta).rem_euclid(count) as usize;
        Self::ALL[next]
    }

    pub fn render(
        self,
        state: &View,
        view: &Entity<View>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        match self {
            Self::Appearance => appearance::render(state, view, window, cx).into_any_element(),
            Self::Viewer => viewer::render(state, view, cx).into_any_element(),
            Self::Export => export::render(state, view, cx).into_any_element(),
        }
    }
}
