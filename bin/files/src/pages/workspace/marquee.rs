#![allow(dead_code)]
//! Blank-space rectangle selection uses native pointer capture and tab-owned identities.

use zgui::prelude::*;

use super::{geometry::GridMotion, ui::Browser};

#[derive(Clone, Copy)]
pub(super) enum Layout {
    List { viewport: NodeRef },
    Grid(GridMotion),
}

impl Browser {
    pub(super) fn marquee_surface(&self, content: AnyView, _layout: Layout) -> AnyView {
        content
    }
}
