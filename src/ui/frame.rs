//! Shared GDS outer CSD window frame, toolbar, footer, drag strip, and workspace wrapper.

mod drag;
mod footer;
mod toolbar;
mod workspace;

pub use drag::{WindowDragArea, WindowDragAreaProps};
pub use footer::{Footer, FooterProps};
pub use toolbar::{Toolbar, ToolbarProps};
pub use workspace::{WorkspaceFrame, WorkspaceFrameProps};

use zgui::prelude::*;
use zgui::{component, css, view};

use crate::app::Gds;
use crate::appearance::tokens;
use crate::ui::feedback::tooltip::GdsTooltipProviderProps;

pub const TOOLBAR_HEIGHT: f32 = tokens::TOOLBAR_HEIGHT;
pub const FOOTER_HEIGHT: f32 = tokens::FOOTER_HEIGHT;

const FRAME_SHEET: &str = "gds-ui-frame";

const FRAME_CSS: &str = css!(
    ".gds-frame {
        position: relative;
        background-color: transparent;
    }
    .gds-frame[data-maximized=\"true\"] {
        border-radius: 0px;
        border-width: 0px;
    }
    .gds-frame__grip {
        position: absolute;
        z-index: 30;
    }
    .gds-frame__grip--n { top: 0px; left: 6px; right: 6px; height: 5px; }
    .gds-frame__grip--s { bottom: 0px; left: 6px; right: 6px; height: 5px; }
    .gds-frame__grip--w { left: 0px; top: 6px; bottom: 6px; width: 5px; }
    .gds-frame__grip--e { right: 0px; top: 6px; bottom: 6px; width: 5px; }
    .gds-frame__grip--nw { top: 0px; left: 0px; width: 10px; height: 10px; }
    .gds-frame__grip--ne { top: 0px; right: 0px; width: 10px; height: 10px; }
    .gds-frame__grip--sw { bottom: 0px; left: 0px; width: 10px; height: 10px; }
    .gds-frame__grip--se { bottom: 0px; right: 0px; width: 10px; height: 10px; }
    .gds-window-drag-area {
        display: flex;
        flex-direction: row;
        align-items: center;
        flex: 1 1 0%;
        min-width: 0px;
        align-self: stretch;
    }
    .gds-toolbar {
        min-width: 0px;
        background-color: transparent;
    }
    .gds-footer {
        min-width: 0px;
        color: var(--gds-foreground);
        font-size: var(--gds-font-size-xs, 12px);
    }
    .gds-workspace-frame {
        display: flex;
        flex-direction: row;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }"
);

pub(super) fn install_frame_stylesheets() {
    crate::ui::styles::install_ui_stylesheets();
    zgui::view::install_stylesheet(FRAME_SHEET, FRAME_CSS);
}

const RESIZE_EDGES: [(ResizeEdge, &str); 8] = [
    (ResizeEdge::North, "gds-frame__grip gds-frame__grip--n"),
    (ResizeEdge::South, "gds-frame__grip gds-frame__grip--s"),
    (ResizeEdge::West, "gds-frame__grip gds-frame__grip--w"),
    (ResizeEdge::East, "gds-frame__grip gds-frame__grip--e"),
    (ResizeEdge::NorthWest, "gds-frame__grip gds-frame__grip--nw"),
    (ResizeEdge::NorthEast, "gds-frame__grip gds-frame__grip--ne"),
    (ResizeEdge::SouthWest, "gds-frame__grip gds-frame__grip--sw"),
    (ResizeEdge::SouthEast, "gds-frame__grip gds-frame__grip--se"),
];

fn resize_cursor(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::North | ResizeEdge::South => CursorStyle::ResizeNorthSouth,
        ResizeEdge::East | ResizeEdge::West => CursorStyle::ResizeEastWest,
        ResizeEdge::NorthWest | ResizeEdge::SouthEast => CursorStyle::ResizeNorthWestSouthEast,
        _ => CursorStyle::ResizeNorthEastSouthWest,
    }
}

#[component]
fn ResizeGrip(edge: ResizeEdge, class: &'static str) -> impl IntoView {
    let Some(window) = zgui::prelude::try_use_window() else {
        return AnyView::new(());
    };
    let cursor = resize_cursor(edge);
    let resize_handler = window.resize_drag_handler(edge);
    let enter_win = window.clone();
    let leave_win = window;
    AnyView::new(view! {
        box(
            class = class,
            a11y:hidden = true,
            on:pointer_down = resize_handler,
            on:pointer_enter = move |_| enter_win.set_cursor(cursor),
            on:pointer_leave = move |_| leave_win.set_cursor(CursorStyle::Default)
        ) {}
    })
}

/// Outer window shell installing `"gds-appearance"` + `"gds-ui-base"` and framing client chrome.
#[component]
pub fn Frame(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    if let Some(gds) = Gds::try_current() {
        let _ = gds.appearance().install_window_sheet();
    }
    install_frame_stylesheets();

    let window = zgui::prelude::try_use_window();
    let is_maximized = Signal::derive_local({
        let window = window.clone();
        move || {
            window
                .as_ref()
                .is_some_and(|w| w.maximized().get() || w.fullscreen().get().is_some())
        }
    });
    let framed = Signal::derive_local(move || !is_maximized.get());

    let content = if zgui_ui::tooltip::TooltipDelays::current().is_none() {
        AnyView::new(view! {
            GdsTooltipProvider {{children.into_view_once()}}
        })
    } else {
        children.into_view_once()
    };

    let grips = window.is_some().then(|| {
        AnyView::new(view! {
            Show(when = move || framed.get()) {
                for item in move || RESIZE_EDGES, key = |item: &(ResizeEdge, &'static str)| item.1 {
                    ResizeGrip(edge = item.0, class = item.1)
                }
            }
        })
    });

    view! {
        column(
            class = "gds-frame",
            attr:data-maximized = move || is_maximized.get().then(|| "true".to_owned()),
            class = class,
            {..attrs}
        ) {
            box(class = "gds-frame-content") {
                {content}
            }
            {grips}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Arc;

    use zgui::app::Fonts;
    use zgui::atlas::{SinkError, TextureFormat, TextureId, TextureSink};
    use zgui::geom::{CssPx, Device, DevicePx, Point, Rect, Size};
    use zgui::platform::{AppHandler, PlatformError, Surface, SurfaceEvent};
    use zgui::prelude::*;
    use zgui::render::{
        ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
        Renderer, TextureHandle,
    };
    use zgui::runtime::AppError;
    use zgui::view;
    use zgui::vocab::{Modifiers, PointerAction, PointerButton, PointerEvent, Timestamp};
    use zgui_platform_headless::Harness;

    use super::{FOOTER_HEIGHT, FrameProps, TOOLBAR_HEIGHT, ToolbarProps, WindowDragAreaProps};
    use crate::appearance::tokens;

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
    fn toolbar_and_footer_heights_match_design_tokens() {
        assert_eq!(TOOLBAR_HEIGHT, tokens::TOOLBAR_HEIGHT);
        assert_eq!(FOOTER_HEIGHT, tokens::FOOTER_HEIGHT);
    }

    #[test]
    fn frame_mounts_unmaximized_grips_and_no_drag_child_clicks_without_panicking() {
        let clicked = Rc::new(Cell::new(0usize));
        let clicked_ref = Rc::clone(&clicked);

        let driver = move |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
            let mut harness = Harness::new(handler);
            harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(640.0),
                DevicePx(480.0),
            )));
            harness.settle(32);

            // Hover over north edge grip to exercise ResizeEdge cursor routing.
            harness.deliver_to_first(SurfaceEvent::Pointer {
                action: PointerAction::Moved,
                event: PointerEvent::mouse(Point::new(CssPx(320.0), CssPx(2.0))),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
            harness.settle(16);

            let surface = harness
                .platform()
                .offscreens()
                .first()
                .cloned()
                .expect("surface");
            assert!(
                surface
                    .cursor_log()
                    .contains(&CursorStyle::ResizeNorthSouth),
                "hovering north edge grip sets vertical resize cursor"
            );

            // Click the no-drag control mounted inside WindowDragArea.
            let button_pos = Point::new(CssPx(40.0), CssPx(26.0));
            harness.deliver_to_first(SurfaceEvent::Pointer {
                action: PointerAction::Pressed,
                event: PointerEvent::mouse(button_pos).with_button(PointerButton::Primary),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
            harness.deliver_to_first(SurfaceEvent::Pointer {
                action: PointerAction::Released,
                event: PointerEvent::mouse(button_pos).with_button(PointerButton::Primary),
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
            .with_size(640.0, 480.0)
            .run_on(driver, move || {
                let win = zgui::prelude::try_use_window();
                let no_drag = win.as_ref().map(WindowHandle::no_drag_handler);
                let clicked = Rc::clone(&clicked_ref);
                view! {
                    Frame {
                        Toolbar {
                            WindowDragArea {
                                control(
                                    style:width = "160px",
                                    style:height = "36px",
                                    on:pointer_down = move |ev: &mut zgui::view::EventCx<'_, zgui::view::events::PointerDown>| {
                                        if let Some(ref handler) = no_drag {
                                            handler(ev);
                                        }
                                    },
                                    on:click = move |_| clicked.set(clicked.get() + 1)
                                ) {"Action"}
                            }
                        }
                    }
                }
            })
            .expect("frame runs headlessly");

        assert_eq!(
            clicked.get(),
            1,
            "no-drag child inside WindowDragArea must receive primary click"
        );
    }
}
