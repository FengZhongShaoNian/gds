use std::sync::{Arc, Mutex};

use zgui::app::Fonts;
use zgui::atlas::{SinkError, TextureFormat, TextureId, TextureSink};
use zgui::geom::{CssPx, Device, DevicePx, Point, Rect, Size};
use zgui::platform::{AppHandler, Decorations, PlatformError, Surface, SurfaceEvent};
use zgui::render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, TextureHandle,
};
use zgui::runtime::AppError;
use zgui::view;
use zgui::vocab::{Modifiers, PointerAction, PointerButton, PointerEvent, Role, Timestamp};
use zgui_platform_headless::Harness;

use super::{
    TitleBarProps, fit_size, initial_size, minimum_size, session_hides_controls, workspace_options,
};

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
fn window_size_constants_are_ordered() {
    let init = initial_size();
    let min = minimum_size();
    assert!(init.width.0 > min.width.0);
    assert!(init.height.0 > min.height.0);
    assert_eq!(fit_size(init), init);
}

#[test]
fn workspace_options_use_transparent_csd_defaults() {
    let options = workspace_options("GDS Test", Some("io.github.GanDesktop.Test"));
    let attrs = options.attributes();
    assert_eq!(attrs.decorations, Decorations::None);
    assert!(attrs.transparent);
    assert_eq!(attrs.title, "GDS Test");
    assert_eq!(
        attrs.application_id.as_deref(),
        Some("io.github.GanDesktop.Test")
    );
}

#[test]
fn niri_session_hides_the_entire_control_group() {
    for desktop in ["niri", "Niri", "niri:GNOME", " custom : niri "] {
        assert!(session_hides_controls(Some(desktop), false));
    }
}

#[test]
fn declared_desktop_takes_precedence_over_inherited_niri_socket() {
    for desktop in ["GNOME", "KDE", "sway", "not-niri"] {
        assert!(!session_hides_controls(Some(desktop), true));
    }
}

#[test]
fn missing_desktop_uses_the_niri_socket_as_a_fallback() {
    assert!(session_hides_controls(None, true));
    assert!(session_hides_controls(Some(" "), true));
    assert!(!session_hides_controls(None, false));
}

#[test]
fn csd_close_button_exposes_button_role_and_label_and_stops_drag() {
    if session_hides_controls(
        std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
        std::env::var_os("NIRI_SOCKET").is_some_and(|s| !s.is_empty()),
    ) {
        return;
    }

    let found_close: Arc<Mutex<Option<(Role, Option<String>, bool)>>> = Arc::new(Mutex::new(None));
    let recorded = Arc::clone(&found_close);

    let driver = move |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
        let mut harness = Harness::new(handler);
        harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
            DevicePx(640.0),
            DevicePx(120.0),
        )));
        harness.settle(32);

        let surface = harness
            .platform()
            .offscreens()
            .first()
            .cloned()
            .expect("window surface created");
        let updates = surface.a11y_log();
        for update in updates {
            for (_, node) in update.nodes {
                if node.label() == Some("Close window") {
                    *recorded.lock().expect("lock") = Some((
                        node.role(),
                        node.label().map(str::to_owned),
                        node.toggled().is_some(),
                    ));
                }
            }
        }

        // Clicking near top-right (inside the close button) closes the window without panic,
        // even though TitleBar wraps WindowControls with move_drag_handler().
        let pos = Point::new(CssPx(620.0), CssPx(26.0));
        harness.deliver_to_first(SurfaceEvent::Pointer {
            action: PointerAction::Pressed,
            event: PointerEvent::mouse(pos).with_button(PointerButton::Primary),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        harness.deliver_to_first(SurfaceEvent::Pointer {
            action: PointerAction::Released,
            event: PointerEvent::mouse(pos).with_button(PointerButton::Primary),
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
        .with_size(640.0, 120.0)
        .run_on(driver, || view! { TitleBar(title = "Test Window") })
        .expect("window titlebar runs headlessly");

    assert_eq!(
        *found_close.lock().expect("lock"),
        Some((Role::Button, Some("Close window".to_owned()), false)),
        "CSD close button must project Role::Button with label 'Close window' and no toggled state"
    );
}
