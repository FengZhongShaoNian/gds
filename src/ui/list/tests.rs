use std::sync::{Arc, Mutex};

use zgui::app::Fonts;
use zgui::atlas::{SinkError, TextureFormat, TextureId, TextureSink};
use zgui::geom::{CssPx, Device, DevicePx, Point, Rect, Size};
use zgui::platform::{AppHandler, PlatformError, Surface, SurfaceEvent};
use zgui::reactive::RwSignal;
use zgui::render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, TextureHandle,
};
use zgui::runtime::AppError;
use zgui::view;
use zgui::vocab::Role;
use zgui_platform_headless::Harness;

use super::{
    DataListStyle, ListGeometry, ListRowProps, ListStyle, ListSurfaceProps,
    VirtualizedListSurfaceProps,
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
fn blank_press_tracks_scrolled_rows_and_preserves_header_and_scrollbars() {
    let geometry = ListGeometry {
        rows: 5,
        row_height: DataListStyle::ROW_HEIGHT,
        header_height: DataListStyle::HEADER_HEIGHT,
        inset_x: 0.0,
        bottom_inset: DataListStyle::SCROLLBAR_LANE,
        scrollbar_width: DataListStyle::SCROLLBAR_LANE,
    };
    let bounds = Rect::new(
        Point::new(CssPx(20.0), CssPx(50.0)),
        Size::new(CssPx(400.0), CssPx(300.0)),
    );
    let hit = |x: f32, y: f32, offset: f32| {
        geometry.is_blank(
            bounds,
            Point::new(CssPx(bounds.origin.x.0 + x), CssPx(bounds.origin.y.0 + y)),
            CssPx(offset),
        )
    };
    assert!(
        !hit(100.0, 180.0, 0.0),
        "a row retains its own selection behavior"
    );
    assert!(
        hit(100.0, 240.0, 0.0),
        "space below the rows clears selection"
    );
    assert!(
        hit(100.0, 180.0, -80.0),
        "the live scroll offset moves the blank boundary"
    );
    assert!(!hit(100.0, 15.0, -80.0), "column headings keep selection");
    assert!(
        !hit(390.0, 240.0, 0.0),
        "the vertical scrollbar keeps selection"
    );
    assert!(
        !hit(100.0, 290.0, 0.0),
        "the horizontal scrollbar keeps selection"
    );
    assert!(
        !hit(100.0, 310.0, 0.0),
        "a press outside this list does not clear it"
    );
}

#[test]
fn virtualized_list_surface_mounts_only_visible_window_of_rows() {
    let built_indices: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
    let built_ref = Arc::clone(&built_indices);
    let selected_item_seen: Arc<Mutex<Option<(Role, Option<bool>)>>> = Arc::new(Mutex::new(None));
    let selected_ref = Arc::clone(&selected_item_seen);

    let driver = move |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
        let mut harness = Harness::new(handler);
        harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
            DevicePx(400.0),
            DevicePx(200.0),
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
                if node.role() == Role::ListItem && node.label() == Some("Row 1") {
                    *selected_ref.lock().expect("lock") = Some((node.role(), node.is_selected()));
                }
            }
        }
        harness.shut_down();
        Ok(())
    };

    zgui::app()
        .with_fonts(Fonts::shipped_only())
        .with_renderer(Box::new(noop_renderer))
        .with_size(400.0, 200.0)
        .run_on(driver, move || {
            let count = RwSignal::new_local(10_000_usize);
            let built = Arc::clone(&built_ref);
            view! {
                column(style:width = "400px", style:height = "200px") {
                    VirtualizedListSurface(
                        count = count,
                        list_style = ListStyle::Striped,
                        label = "Files list",
                        row = move |index: usize| {
                            built.lock().expect("lock").push(index);
                            let selected = index == 1;
                            view! {
                                ListRow(
                                    index = index,
                                    selected = selected,
                                    a11y:label = format!("Row {index}")
                                ) {
                                    text {{format!("Row {index}")}}
                                }
                            }
                        }
                    )
                }
            }
        })
        .expect("virtualized list runs headlessly");

    let built = built_indices.lock().expect("lock");
    assert!(!built.is_empty());
    assert!(
        built.len() < 30,
        "virtualized 10,000-row list in 200px viewport should only build visible rows + overscan, built {}",
        built.len()
    );
    assert_eq!(
        *selected_item_seen.lock().expect("lock"),
        Some((Role::ListItem, Some(true)))
    );

    // Also verify ListSurface mounts cleanly on its own.
    let _ = ListStyle::Plain;
    let _plain = view! { ListSurface { text { "Plain item" } } };
}
