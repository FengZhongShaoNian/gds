//! Shared 160ms transition clocks and responsive viewport geometry helpers.

use std::time::Duration;

use gds::{
    appearance::tokens::{FOOTER_HEIGHT, TOOLBAR_HEIGHT},
    ui::sidebar::{SideBar, SidebarPresentation, SidebarState},
};
use gpui_kit::{
    App, Window,
    base::motion::{self, Transition},
};

use crate::{
    entities::preferences::{MAX_INSPECTOR_WIDTH, MIN_INSPECTOR_WIDTH},
    shared::geometry::Size2,
};

const PANEL_ANIM_DURATION: Duration = Duration::from_millis(160);
const MIN_VIEWPORT_WIDTH: f32 = 240.0;
const MIN_VIEWPORT_HEIGHT: f32 = 160.0;

pub(in crate::pages::workspace::ui) fn sidebar_presentation(
    state: SidebarState,
    window: &mut Window,
    cx: &mut App,
) -> SidebarPresentation {
    SideBar::default()
        .state(state)
        .sample("images-left-sidebar", window, cx)
}

pub(in crate::pages::workspace::ui) fn effective_inspector_width(
    window_width: f32,
    left_width: f32,
    persisted: u16,
) -> f32 {
    let available =
        (window_width - left_width - MIN_VIEWPORT_WIDTH).max(f32::from(MIN_INSPECTOR_WIDTH));
    let upper = f32::from(MAX_INSPECTOR_WIDTH).min(available);
    f32::from(persisted).clamp(f32::from(MIN_INSPECTOR_WIDTH), upper)
}

pub(in crate::pages::workspace::ui) fn inspector_presentation(
    visible: bool,
    resizing: bool,
    target_width: f32,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    let target = if visible { target_width } else { 0.0 };
    let duration = if resizing {
        Duration::ZERO
    } else {
        PANEL_ANIM_DURATION
    };
    motion::transition(
        "images-right-inspector",
        target,
        Transition::new(duration),
        window,
        cx,
    )
    .max(0.0)
}

pub(in crate::pages::workspace::ui) fn compute_viewport_size(
    window_width: f32,
    window_height: f32,
    left_width: f32,
    inspector_width: f32,
) -> Size2 {
    Size2::new(
        (window_width - left_width - inspector_width).max(MIN_VIEWPORT_WIDTH),
        (window_height - TOOLBAR_HEIGHT - FOOTER_HEIGHT).max(MIN_VIEWPORT_HEIGHT),
    )
}
