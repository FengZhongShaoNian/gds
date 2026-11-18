//! Navigation list items, pinned badges, and hover/corner action slots.

mod action;
mod actions;
mod content;
mod details;
mod header;
mod icon;
mod label;

pub use action::{
    CornerAction, CornerActionProps, CornerButton, CornerButtonProps, HoverAction,
    HoverActionProps, corner_action, corner_button, hover_action,
};
pub use actions::{NavItemActions, NavItemActionsProps};
pub use content::{NavItemContent, NavItemContentProps};
pub use details::{NavItemDetails, NavItemDetailsProps};
pub use header::{NavItemHeader, NavItemHeaderProps};
pub use icon::{PinnedIcon, PinnedIconProps, WithPin, WithPinProps, pinned_icon, with_pin};
pub use label::{NavItemLabel, NavItemLabelProps};

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, css, view};

const NAVIGATION_SHEET: &str = "gds-ui-navigation";

const NAVIGATION_CSS: &str = css!(
    ".gds-nav-item {
        position: relative;
        width: 100%;
        max-width: 100%;
        min-width: 0px;
        box-sizing: border-box;
        overflow: hidden;
        padding: 0px 10px;
        user-select: none;
        transition:
            background-color var(--gds-motion-fast, 100ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1)),
            color var(--gds-motion-fast, 100ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }
    .gds-nav-item:active {
        background-color: var(--gds-pressed);
    }
    .gds-nav-item[data-selected=\"true\"]:hover {
        background-color: var(--gds-selection);
        color: var(--gds-selection-foreground);
    }
    .gds-nav-item[data-drop-target=\"true\"] {
        background-color: var(--gds-drag-target);
        outline: 1px solid var(--gds-primary);
        outline-offset: -1px;
    }
    .gds-nav-item:focus-visible {
        outline: 1px solid var(--gds-primary);
        outline-offset: -1px;
    }
    .gds-nav-item__content {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        max-width: 100%;
        min-width: 0px;
        overflow: hidden;
        gap: 2px;
    }
    .gds-nav-item__header {
        display: flex;
        flex-direction: row;
        align-items: center;
        flex: 1 1 0%;
        width: 100%;
        max-width: 100%;
        min-width: 0px;
        overflow: hidden;
        gap: 8px;
    }
    .gds-nav-item__label {
        flex: 1 1 0%;
        min-width: 0px;
        max-width: 100%;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: var(--gds-font-size-small, 14px);
    }
    .gds-nav-item__details {
        width: 100%;
        max-width: 100%;
        min-width: 0px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-muted-foreground);
    }
    .gds-nav-item[data-selected=\"true\"] .gds-nav-item__details {
        color: inherit;
        opacity: 0.85;
    }
    .gds-nav-item__actions {
        display: flex;
        flex-direction: row;
        align-items: center;
        flex-shrink: 0;
        gap: 2px;
    }
    .gds-sidebar[data-collapsed=\"true\"] .gds-nav-item__label,
    .gds-sidebar[data-collapsed=\"true\"] .gds-nav-item__details,
    .gds-sidebar[data-collapsed=\"true\"] .gds-nav-item__actions {
        display: none;
        pointer-events: none;
    }
    .gds-hover-slot__status {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 100%;
        height: 100%;
        transition: opacity var(--gds-motion-fast, 100ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }
    .gds-hover-slot__action {
        pointer-events: none;
        transition: opacity var(--gds-motion-fast, 100ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }
    .gds-hover-slot[data-has-action=\"true\"]:hover .gds-hover-slot__action,
    .gds-hover-slot[data-has-action=\"true\"][data-revealed=\"true\"] .gds-hover-slot__action,
    .gds-nav-item:hover .gds-hover-slot[data-has-action=\"true\"] .gds-hover-slot__action,
    .gds-nav-item:focus-within .gds-hover-slot[data-has-action=\"true\"] .gds-hover-slot__action {
        opacity: 1;
        pointer-events: auto;
    }
    .gds-hover-slot:not([data-has-action=\"true\"]):hover .gds-hover-slot__status,
    .gds-hover-slot:not([data-has-action=\"true\"])[data-revealed=\"true\"] .gds-hover-slot__status,
    .gds-nav-item:hover .gds-hover-slot:not([data-has-action=\"true\"]) .gds-hover-slot__status {
        opacity: 1;
    }
    .gds-corner-action {
        pointer-events: none;
        z-index: 2;
        transition: opacity var(--gds-motion-fast, 100ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }
    .gds-corner-action[data-revealed=\"true\"],
    .gds-nav-item:hover .gds-corner-action,
    .gds-nav-item:focus-within .gds-corner-action {
        opacity: 1;
        pointer-events: auto;
    }
    .gds-corner-button {
        width: 16px;
        height: 16px;
        min-width: 16px;
        min-height: 16px;
        padding: 0px;
    }
    .gds-corner-button .gds-icon {
        width: 11px;
        height: 11px;
    }"
);

pub(super) fn install_navigation_stylesheets() {
    crate::ui::styles::install_ui_stylesheets();
    zgui::view::install_stylesheet(NAVIGATION_SHEET, NAVIGATION_CSS);
}

/// Interactive navigation row with selected and external/internal drop-target styling.
#[component]
pub fn NavItem(
    #[prop(into, default = Signal::stored_local(false))] selected: Signal<bool, LocalStorage>,
    #[prop(into, default = Signal::stored_local(false))] drop_target: Signal<bool, LocalStorage>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    install_navigation_stylesheets();
    view! {
        row(
            class = "gds-nav-item",
            node_ref = node_ref.unwrap_or_default(),
            tabindex = Focus::Sequential,
            attr:data-selected = move || selected.get().then(|| "true".to_owned()),
            attr:data-drop-target = move || drop_target.get().then(|| "true".to_owned()),
            a11y:role = Role::Button,
            a11y:selected = move || selected.get(),
            class = class,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    use zgui::app::Fonts;
    use zgui::atlas::{SinkError, TextureFormat, TextureId, TextureSink};
    use zgui::geom::{CssPx, Device, DevicePx, Point, Rect, Size};
    use zgui::platform::{AppHandler, PlatformError, Surface, SurfaceEvent};
    use zgui::reactive::{RwSignal, UnsyncCallback};
    use zgui::render::{
        ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
        Renderer, TextureHandle,
    };
    use zgui::runtime::AppError;
    use zgui::view;
    use zgui::vocab::{Modifiers, PointerAction, PointerButton, PointerEvent, Role, Timestamp};
    use zgui_platform_headless::Harness;

    use super::{
        NavItemActionsProps, NavItemContentProps, NavItemHeaderProps, NavItemLabelProps,
        NavItemProps, corner_action, corner_button, hover_action, pinned_icon,
    };
    use crate::ui::icon::IconName;

    struct NoopSink;

    impl TextureSink for NoopSink {
        fn create_texture(
            &mut self,
            _: TextureId,
            _: zgui::geom::Size<i32, Device>,
            _: TextureFormat,
        ) -> Result<(), SinkError> {
            Ok(())
        }

        fn write_texture(
            &mut self,
            _: TextureId,
            _: Rect<i32, Device>,
            _: TextureFormat,
            _: &[u8],
        ) -> Result<(), SinkError> {
            Ok(())
        }

        fn destroy_texture(&mut self, _: TextureId) {}
    }

    struct NoopRenderer {
        target: Option<RenderTarget>,
        sink: NoopSink,
    }

    impl Renderer for NoopRenderer {
        fn capabilities(&self) -> RenderCapabilities {
            RenderCapabilities::MINIMAL
        }

        fn configure(&mut self, target: RenderTarget) {
            self.target = Some(target);
        }

        fn target(&self) -> Option<RenderTarget> {
            self.target
        }

        fn draw(&mut self, _: &zgui::scene::Scene, _: &zgui::bits::DamageSet) -> FrameOutcome {
            FrameOutcome::Presented(FrameStats::default())
        }

        fn register_external(&mut self, _: ExternalTexture) -> TextureHandle {
            TextureHandle(1)
        }

        fn release_external(&mut self, _: TextureHandle) {}

        fn memory(&self) -> MemoryReport {
            MemoryReport::ZERO
        }

        fn texture_sink(&mut self) -> &mut dyn TextureSink {
            &mut self.sink
        }
    }

    fn noop_renderer(
        _: &Arc<dyn Surface>,
        target: RenderTarget,
    ) -> Result<Box<dyn Renderer>, AppError> {
        Ok(Box::new(NoopRenderer {
            target: Some(target),
            sink: NoopSink,
        }))
    }

    #[test]
    fn nav_item_projects_selection_and_corner_button_stops_click_propagation() {
        let row_clicks = Rc::new(Cell::new(0usize));
        let corner_clicks = Rc::new(Cell::new(0usize));
        let seen_a11y: Arc<Mutex<Option<(Role, Option<bool>)>>> = Arc::new(Mutex::new(None));

        let row_clicks_ref = Rc::clone(&row_clicks);
        let corner_clicks_ref = Rc::clone(&corner_clicks);
        let seen_a11y_ref = Arc::clone(&seen_a11y);

        let driver = move |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
            let mut harness = Harness::new(handler);
            harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(240.0),
                DevicePx(120.0),
            )));
            harness.settle(32);

            let surface = harness
                .platform()
                .offscreens()
                .first()
                .cloned()
                .expect("surface");
            for update in surface.a11y_log() {
                for (_, node) in update.nodes {
                    if node.role() == Role::Button && node.label() == Some("Inbox") {
                        *seen_a11y_ref.lock().expect("lock") =
                            Some((node.role(), node.is_selected()));
                    }
                }
            }

            // Click the revealed corner close button at top-right of the 240x38 item.
            let corner_pos = Point::new(CssPx(230.0), CssPx(9.0));
            harness.deliver_to_first(SurfaceEvent::Pointer {
                action: PointerAction::Pressed,
                event: PointerEvent::mouse(corner_pos).with_button(PointerButton::Primary),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
            harness.deliver_to_first(SurfaceEvent::Pointer {
                action: PointerAction::Released,
                event: PointerEvent::mouse(corner_pos).with_button(PointerButton::Primary),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
            harness.settle(16);
            harness.shut_down();
            Ok(())
        };

        zgui::app()
            .with_fonts(Fonts::shipped_only())
            .with_renderer(Box::new(noop_renderer))
            .with_size(240.0, 120.0)
            .run_on(driver, move || {
                let row_clicks = Rc::clone(&row_clicks_ref);
                let corner_clicks = Rc::clone(&corner_clicks_ref);
                let revealed = RwSignal::new_local(true);
                let close_btn = corner_button(
                    IconName::X,
                    "Close tab",
                    UnsyncCallback::new(move |()| corner_clicks.set(corner_clicks.get() + 1)),
                );
                view! {
                    column(style:width = "240px") {
                        NavItem(
                            selected = true,
                            drop_target = true,
                            a11y:label = "Inbox",
                            on:click = move |_| row_clicks.set(row_clicks.get() + 1)
                        ) {
                            NavItemContent {
                                NavItemHeader {
                                    {pinned_icon(IconName::Folder, true)}
                                    NavItemLabel { "Inbox" }
                                }
                            }
                            NavItemActions {
                                {hover_action(view! { text { "2" } }, None)}
                            }
                            {corner_action(close_btn, revealed.into())}
                        }
                    }
                }
            })
            .expect("nav item runs headlessly");

        assert_eq!(
            *seen_a11y.lock().expect("lock"),
            Some((Role::Button, Some(true)))
        );
        assert_eq!(corner_clicks.get(), 1);
        assert_eq!(row_clicks.get(), 0);
    }
}
