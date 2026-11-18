//! Suite window defaults, titlebars, and platform-aware window controls.

#[cfg(test)]
mod tests;

use zgui::geom::{Css, CssPx, Point, Size};
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::PointerButton;
use zgui::{component, view};

use crate::app::Gds;
use crate::appearance::tokens;
use crate::ui::button::IconButtonProps;
use crate::ui::icon::IconName;

pub const HEIGHT: f32 = tokens::TOOLBAR_HEIGHT;
pub const MACOS_CONTROLS_WIDTH: f32 = 80.;
const INITIAL_WIDTH: f32 = 960.;
const INITIAL_HEIGHT: f32 = 640.;
const DISPLAY_FRACTION: f32 = 0.85;
const MIN_WIDTH: f32 = 520.;
const MIN_HEIGHT: f32 = 360.;

#[must_use]
pub fn minimum_size() -> Size<CssPx, Css> {
    Size::new(CssPx(MIN_WIDTH), CssPx(MIN_HEIGHT))
}

#[must_use]
pub fn initial_size() -> Size<CssPx, Css> {
    Size::new(CssPx(INITIAL_WIDTH), CssPx(INITIAL_HEIGHT))
}

/// Clamp `requested` within `DISPLAY_FRACTION` of the primary monitor when available.
#[must_use]
pub fn fit_size(requested: Size<CssPx, Css>) -> Size<CssPx, Css> {
    if !zgui::reactive::is_ui_thread() {
        return requested;
    }
    let Some(monitors) = zgui::prelude::try_use_monitors() else {
        return requested;
    };
    let list = monitors.get_untracked();
    let Some(primary) = list.first() else {
        return requested;
    };
    let (avail_w, avail_h) = if let Some(logical) = primary.logical {
        (logical.size.width.0, logical.size.height.0)
    } else {
        let scale = primary
            .fractional_scale
            .unwrap_or(primary.scale_factor)
            .max(1.0) as f32;
        (primary.size.width.0 / scale, primary.size.height.0 / scale)
    };
    Size::new(
        CssPx(requested.width.0.min(avail_w * DISPLAY_FRACTION)),
        CssPx(requested.height.0.min(avail_h * DISPLAY_FRACTION)),
    )
}

/// Defaults for GDS client-side-decorated transparent workspace windows.
#[must_use]
pub fn workspace_options(title: impl Into<String>, app_id: Option<&str>) -> WindowOptions {
    let size = fit_size(initial_size());
    let min = minimum_size();
    let mut options = WindowOptions::new(title.into())
        .with_size(size.width.0, size.height.0)
        .with_min_size(min.width.0, min.height.0)
        .with_decorations(Decorations::None)
        .with_transparent(true);
    if let Some(id) = app_id {
        options = options.with_application_id(id);
    }
    options
}

/// Compatibility alias matching legacy `window_options()` callers.
#[must_use]
pub fn window_options() -> WindowOptions {
    workspace_options("", None)
}

/// Temporary Phase 2/3B shim until `GanDesktop/zgui` Commit 2 implements `xdg-foreign-v2` + `xdg-dialog-v1`.
pub fn set_external_parent(window: &WindowHandle, parent: &str, modal: bool) {
    tracing::debug!(%parent, modal, "set_external_parent shim falling back to window focus");
    window.focus();
}

/// Temporary Phase 2/3B shim until `GanDesktop/zgui` Commit 2 wires `xdg_activation_v1`.
pub fn activate_with_token(window: &WindowHandle, token: &str) {
    tracing::debug!(%token, "activate_with_token shim falling back to focus/attention");
    window.focus();
    window.request_attention(true);
}

/// Temporary Phase 2/3B shim until `GanDesktop/zgui` Commit 2 exposes `XdgToplevel::show_window_menu`.
pub fn show_window_menu(window: &WindowHandle, pos: Point<CssPx, Css>) {
    let _ = window;
    tracing::debug!(
        x = pos.x.0,
        y = pos.y.0,
        "show_window_menu is unavailable on this zgui revision"
    );
}

fn hidden_in_current_session() -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    match desktop
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        Some(desktop) => desktop
            .split(':')
            .any(|name| name.trim().eq_ignore_ascii_case("niri")),
        None => std::env::var_os("NIRI_SOCKET").is_some_and(|socket| !socket.is_empty()),
    }
}

/// Client-side window control buttons (minimize, maximize/restore, close), suppressed under Niri.
#[component]
pub fn WindowControls(#[prop(into, optional)] class: Classes) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    if hidden_in_current_session() {
        return AnyView::new(());
    }
    let window = zgui::prelude::try_use_window();
    let max_icon = Signal::derive_local({
        let window = window.clone();
        move || {
            if window.as_ref().is_some_and(|w| w.maximized().get()) {
                IconName::WindowRestore
            } else {
                IconName::WindowMaximize
            }
        }
    });
    let max_label = Signal::derive_local({
        let window = window.clone();
        move || {
            let id = if window.as_ref().is_some_and(|w| w.maximized().get()) {
                "window-restore"
            } else {
                "window-maximize"
            };
            Gds::try_current().map_or_else(|| id.to_owned(), |g| g.i18n().text(id))
        }
    });
    let min_label = Signal::derive_local(|| {
        Gds::try_current().map_or_else(
            || "Minimize".to_owned(),
            |g| g.i18n().text("window-minimize"),
        )
    });
    let close_label = Signal::derive_local(|| {
        Gds::try_current().map_or_else(
            || "Close window".to_owned(),
            |g| g.i18n().text("window-close"),
        )
    });
    let no_drag_min = window.as_ref().map(WindowHandle::no_drag_handler);
    let no_drag_max = window.as_ref().map(WindowHandle::no_drag_handler);
    let no_drag_close = window.as_ref().map(WindowHandle::no_drag_handler);

    let win_min = window.clone();
    let win_max = window.clone();
    let win_close = window;

    AnyView::new(view! {
        row(class = "gds-window-controls", class = class) {
            IconButton(
                icon = IconName::WindowMinimize,
                label = min_label,
                on:pointer_down = move |ev: &mut zgui::view::EventCx<'_, zgui::view::events::PointerDown>| {
                    if let Some(ref handler) = no_drag_min { handler(ev); }
                },
                on:click = move |_| { if let Some(ref w) = win_min { w.minimize(); } }
            )
            IconButton(
                icon = max_icon,
                label = max_label,
                on:pointer_down = move |ev: &mut zgui::view::EventCx<'_, zgui::view::events::PointerDown>| {
                    if let Some(ref handler) = no_drag_max { handler(ev); }
                },
                on:click = move |_| { if let Some(ref w) = win_max { w.toggle_maximized(); } }
            )
            IconButton(
                icon = IconName::WindowClose,
                label = close_label,
                on:pointer_down = move |ev: &mut zgui::view::EventCx<'_, zgui::view::events::PointerDown>| {
                    if let Some(ref handler) = no_drag_close { handler(ev); }
                },
                on:click = move |_| { if let Some(ref w) = win_close { w.close(); } }
            )
        }
    })
}

/// Draggable titlebar strip with optional leading/trailing slots and window menu on secondary click.
#[component]
pub fn TitleBar(
    #[prop(into)] title: Signal<String, LocalStorage>,
    #[prop(optional)] leading: Option<Children>,
    #[prop(optional)] trailing: Option<Children>,
    #[prop(into, optional)] class: Classes,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    let window = zgui::prelude::try_use_window();
    let drag_handler = window.as_ref().map(WindowHandle::move_drag_handler);
    let menu_win = window;

    view! {
        row(
            class = "gds-titlebar gds-toolbar",
            class = class,
            on:pointer_down = move |ev: &mut zgui::view::EventCx<'_, zgui::view::events::PointerDown>| {
                if ev.button == Some(PointerButton::Secondary) {
                    if let Some(ref w) = menu_win {
                        show_window_menu(w, ev.position);
                    }
                } else if let Some(ref handler) = drag_handler {
                    handler(ev);
                }
            }
        ) {
            {leading.map(Children::into_view_once)}
            text(class = "gds-titlebar__title") {{move || title.get()}}
            spacer() {}
            {trailing.map(Children::into_view_once)}
            WindowControls()
        }
    }
}
