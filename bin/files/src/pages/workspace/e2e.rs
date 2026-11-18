//! Driven E2E harness entry point for headless/offscreen workspace verification.

use std::process::ExitCode;

#[cfg(not(feature = "files-e2e"))]
pub(crate) fn run() -> ExitCode {
    eprintln!("gds-files built without `files-e2e`; rebuild with `--features files-e2e`");
    ExitCode::FAILURE
}

#[cfg(feature = "files-e2e")]
pub(crate) fn run() -> ExitCode {
    match driven::execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Files E2E driven runner failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(feature = "files-e2e")]
mod driven {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::fmt::Write as _;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, bail};
    use gds::app::Gds;
    use gds::i18n::Language;
    use gds::service::task_manager::Supervisor;
    use zgui::app::app;
    use zgui::atlas::{MemorySink, TextureId, TextureKind, TextureSink};
    use zgui::bits::DamageSet;
    use zgui::geom::{Css, CssPx, Point};
    use zgui::platform::{AppHandler, PlatformError, SurfaceEvent};
    use zgui::render::{
        ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
        Renderer, TextureHandle,
    };
    use zgui::scene::{Paint, PaintId, PaintKind, Scene};
    use zgui::vocab::{
        ImeEvent, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction,
        PointerButton, PointerEvent, Role, Timestamp,
    };
    use zgui_platform_headless::{Harness, OffscreenSurface};

    use crate::entities::file::ViewMode;
    use crate::entities::location::{Location, places::Places, scheme::TagAddress, volumes};
    use crate::entities::preferences::{Preferences, Service as PreferencesService};
    use crate::pages::workspace::ui::{Browser, LaunchOptions};
    use crate::shared::i18n as files_i18n;

    const WINDOW_WIDTH: f32 = 1280.0;
    const WINDOW_HEIGHT: f32 = 900.0;

    #[derive(Default)]
    struct CaptureState {
        latest_rgba: Option<image::RgbaImage>,
        frames: u64,
    }

    #[derive(Default)]
    struct CaptureRenderer {
        target: Option<RenderTarget>,
        atlas: MemorySink,
        capture: Rc<RefCell<CaptureState>>,
    }

    impl CaptureRenderer {
        fn with_state(capture: Rc<RefCell<CaptureState>>) -> Self {
            Self {
                target: None,
                atlas: MemorySink::new(),
                capture,
            }
        }
    }

    fn blend_premultiplied(dst: &mut [u8; 4], src: [f32; 4]) {
        let sa = src[3].clamp(0.0, 1.0);
        if sa <= 0.0 {
            return;
        }
        let inv = 1.0 - sa;
        let out_r = (src[0].clamp(0.0, 1.0) * 255.0 + f32::from(dst[0]) * inv).round();
        let out_g = (src[1].clamp(0.0, 1.0) * 255.0 + f32::from(dst[1]) * inv).round();
        let out_b = (src[2].clamp(0.0, 1.0) * 255.0 + f32::from(dst[2]) * inv).round();
        let out_a = (sa * 255.0 + f32::from(dst[3]) * inv).round();
        dst[0] = out_r.clamp(0.0, 255.0) as u8;
        dst[1] = out_g.clamp(0.0, 255.0) as u8;
        dst[2] = out_b.clamp(0.0, 255.0) as u8;
        dst[3] = out_a.clamp(0.0, 255.0) as u8;
    }

    fn fill_rect(img: &mut image::RgbaImage, bounds: [f32; 4], color: [f32; 4]) {
        if color[3] <= 0.0 || bounds[2] <= 0.0 || bounds[3] <= 0.0 {
            return;
        }
        let max_w = i32::try_from(img.width()).unwrap_or(i32::MAX);
        let max_h = i32::try_from(img.height()).unwrap_or(i32::MAX);
        let x0 = (bounds[0].floor() as i32).clamp(0, max_w);
        let y0 = (bounds[1].floor() as i32).clamp(0, max_h);
        let x1 = ((bounds[0] + bounds[2]).ceil() as i32).clamp(0, max_w);
        let y1 = ((bounds[1] + bounds[3]).ceil() as i32).clamp(0, max_h);
        for y in y0..y1 {
            for x in x0..x1 {
                let px = img.get_pixel_mut(x as u32, y as u32);
                blend_premultiplied(&mut px.0, color);
            }
        }
    }

    fn texture_id_from_word(word: u32) -> Option<TextureId> {
        let kind = match word >> 16 {
            0 => TextureKind::Mono,
            1 => TextureKind::Subpixel,
            2 => TextureKind::Color,
            3 => TextureKind::Image,
            _ => return None,
        };
        Some(TextureId::new(kind, word & 0xffff))
    }

    fn paint_solid_color(scene: &Scene, kind: u32, index: u32) -> Option<[f32; 4]> {
        if kind != PaintKind::Solid as u32 {
            return None;
        }
        match scene.paints.get(PaintId(index)) {
            Some(Paint::Solid(color)) => Some(color.to_premultiplied_srgb()),
            _ => None,
        }
    }

    impl Renderer for CaptureRenderer {
        fn capabilities(&self) -> RenderCapabilities {
            RenderCapabilities::MINIMAL
        }

        fn configure(&mut self, target: RenderTarget) {
            self.target = Some(target);
        }

        fn target(&self) -> Option<RenderTarget> {
            self.target
        }

        fn draw(&mut self, scene: &Scene, _damage: &DamageSet) -> FrameOutcome {
            let size = self
                .target
                .map(|target| target.size)
                .unwrap_or_else(|| scene.viewport());
            let width = u32::try_from(size.width.max(1)).unwrap_or(1);
            let height = u32::try_from(size.height.max(1)).unwrap_or(1);
            let mut img =
                image::RgbaImage::from_pixel(width, height, image::Rgba([24, 24, 28, 255]));

            #[derive(Clone, Copy)]
            enum DrawItem {
                Quad(usize, u32),
                Mono(usize, u32),
            }

            let mut items = Vec::with_capacity(
                scene.primitives.quads.len() + scene.primitives.mono_sprites.len(),
            );
            for (idx, quad) in scene.primitives.quads.iter().enumerate() {
                items.push(DrawItem::Quad(idx, quad.order));
            }
            for (idx, sprite) in scene.primitives.mono_sprites.iter().enumerate() {
                items.push(DrawItem::Mono(idx, sprite.order));
            }
            items.sort_by_key(|item| match *item {
                DrawItem::Quad(idx, order) => (order, 0_u8, idx),
                DrawItem::Mono(idx, order) => (order, 1_u8, idx),
            });

            for item in items {
                match item {
                    DrawItem::Quad(idx, _) => {
                        let quad = &scene.primitives.quads[idx];
                        if let Some(fill) =
                            paint_solid_color(scene, quad.fill.kind, quad.fill.index)
                        {
                            fill_rect(&mut img, quad.bounds, fill);
                        }
                        if let Some(stroke) =
                            paint_solid_color(scene, quad.stroke.kind, quad.stroke.index)
                        {
                            let [x, y, w, h] = quad.bounds;
                            let [t, r, b, l] = quad.border;
                            if t > 0.0 {
                                fill_rect(&mut img, [x, y, w, t], stroke);
                            }
                            if b > 0.0 {
                                fill_rect(&mut img, [x, y + h - b, w, b], stroke);
                            }
                            if l > 0.0 {
                                fill_rect(&mut img, [x, y, l, h], stroke);
                            }
                            if r > 0.0 {
                                fill_rect(&mut img, [x + w - r, y, r, h], stroke);
                            }
                        }
                    }
                    DrawItem::Mono(idx, _) => {
                        let sprite = &scene.primitives.mono_sprites[idx];
                        if sprite.tile.is_unresolved() {
                            continue;
                        }
                        let Some(tex_id) = texture_id_from_word(sprite.tile.texture) else {
                            continue;
                        };
                        let [tx, ty, tw, th] = sprite.tile.bounds;
                        let [dx, dy, dw, dh] = sprite.bounds;
                        if tw <= 0 || th <= 0 || dw <= 0.0 || dh <= 0.0 {
                            continue;
                        }
                        let max_px_x = i32::try_from(width).unwrap_or(i32::MAX);
                        let max_px_y = i32::try_from(height).unwrap_or(i32::MAX);
                        let start_x = (dx.floor() as i32).clamp(0, max_px_x);
                        let start_y = (dy.floor() as i32).clamp(0, max_px_y);
                        let end_x = ((dx + dw).ceil() as i32).clamp(0, max_px_x);
                        let end_y = ((dy + dh).ceil() as i32).clamp(0, max_px_y);
                        for py in start_y..end_y {
                            let v = ((py as f32 + 0.5 - dy) / dh) * th as f32;
                            let sy = ty + (v.floor() as i32).clamp(0, th - 1);
                            for px in start_x..end_x {
                                let u = ((px as f32 + 0.5 - dx) / dw) * tw as f32;
                                let sx = tx + (u.floor() as i32).clamp(0, tw - 1);
                                if let Some(cov_bytes) = self.atlas.texel(tex_id, sx, sy)
                                    && let Some(&cov_u8) = cov_bytes.first()
                                    && cov_u8 > 0
                                {
                                    let cov = f32::from(cov_u8) / 255.0;
                                    let tinted = [
                                        sprite.color[0] * cov,
                                        sprite.color[1] * cov,
                                        sprite.color[2] * cov,
                                        sprite.color[3] * cov,
                                    ];
                                    let px_dst = img.get_pixel_mut(px as u32, py as u32);
                                    blend_premultiplied(&mut px_dst.0, tinted);
                                }
                            }
                        }
                    }
                }
            }

            {
                let mut guard = self.capture.borrow_mut();
                guard.latest_rgba = Some(img);
                guard.frames = guard.frames.saturating_add(1);
            }

            FrameOutcome::Presented(FrameStats {
                vector_passes: 0,
                draw_calls: u32::try_from(
                    scene.primitives.quads.len() + scene.primitives.mono_sprites.len(),
                )
                .unwrap_or(u32::MAX),
                damage_px: u64::from(width) * u64::from(height),
                bytes_uploaded: 0,
                memory: MemoryReport::ZERO,
            })
        }

        fn register_external(&mut self, _texture: ExternalTexture) -> TextureHandle {
            TextureHandle(0)
        }

        fn release_external(&mut self, _handle: TextureHandle) {}

        fn memory(&self) -> MemoryReport {
            MemoryReport::ZERO
        }

        fn texture_sink(&mut self) -> &mut dyn TextureSink {
            &mut self.atlas
        }
    }

    struct NodeEntry {
        role: Role,
        label: Option<String>,
        value: Option<String>,
        bounds: Option<(f64, f64, f64, f64)>,
        children: Vec<u64>,
        disabled: bool,
    }

    #[derive(Default)]
    struct SemanticStore {
        root: Option<u64>,
        focus: Option<u64>,
        nodes: HashMap<u64, NodeEntry>,
    }

    impl SemanticStore {
        fn ingest(&mut self, surface: &OffscreenSurface) {
            for update in surface.take_a11y_log() {
                if let Some(tree) = update.tree {
                    self.root = Some(tree.root.0);
                }
                self.focus = Some(update.focus.0);
                for (id, node) in update.nodes {
                    let bounds = node
                        .bounds()
                        .map(|rect| (rect.x0, rect.y0, rect.x1, rect.y1));
                    let children = node.children().iter().map(|child| child.0).collect();
                    self.nodes.insert(
                        id.0,
                        NodeEntry {
                            role: node.role(),
                            label: node.label().map(str::to_owned),
                            value: node.value().map(str::to_owned),
                            bounds,
                            children,
                            disabled: node.is_disabled(),
                        },
                    );
                }
            }
        }

        fn dump(&self) -> String {
            let mut out = String::new();
            let Some(root) = self.root else {
                return String::from("(empty accessibility tree)\n");
            };
            let _ = writeln!(out, "focus={:?}", self.focus);
            self.write_node(&mut out, root, 0, &mut Vec::new());
            out
        }

        fn write_node(&self, out: &mut String, id: u64, depth: usize, visited: &mut Vec<u64>) {
            if visited.contains(&id) {
                return;
            }
            visited.push(id);
            let Some(node) = self.nodes.get(&id) else {
                return;
            };
            let indent = "  ".repeat(depth);
            let _ = write!(out, "{indent}- {:?} (id={id})", node.role);
            if let Some(label) = &node.label {
                let _ = write!(out, " label={label:?}");
            }
            if let Some(value) = &node.value {
                let _ = write!(out, " value={value:?}");
            }
            if let Some((x0, y0, x1, y1)) = node.bounds {
                let _ = write!(out, " bounds=[{x0:.1},{y0:.1},{x1:.1},{y1:.1}]");
            }
            if node.disabled {
                let _ = write!(out, " [disabled]");
            }
            let _ = writeln!(out);
            for &child in &node.children {
                self.write_node(out, child, depth + 1, visited);
            }
        }

        fn find_center_containing(
            &self,
            needle: &str,
            role: Option<Role>,
        ) -> Option<Point<CssPx, Css>> {
            self.nodes.values().find_map(|node| {
                if role.is_some_and(|expected| node.role != expected) {
                    return None;
                }
                let matches_label = node
                    .label
                    .as_deref()
                    .is_some_and(|label| label.contains(needle));
                let matches_value = node
                    .value
                    .as_deref()
                    .is_some_and(|value| value.contains(needle));
                if !(matches_label || matches_value) {
                    return None;
                }
                let (x0, y0, x1, y1) = node.bounds?;
                Some(Point::new(
                    CssPx(((x0 + x1) * 0.5) as f32),
                    CssPx(((y0 + y1) * 0.5) as f32),
                ))
            })
        }
    }

    fn pump_frames(harness: &mut Harness<Box<dyn AppHandler>>) {
        for _ in 0..4 {
            harness.pump();
        }
    }

    fn send_click(
        harness: &mut Harness<Box<dyn AppHandler>>,
        pos: Point<CssPx, Css>,
        button: PointerButton,
        ts: &Cell<u64>,
    ) {
        let now = Timestamp::from_origin(Duration::from_millis(ts.get()));
        ts.set(ts.get() + 16);
        for (action, event) in [
            (PointerAction::Moved, PointerEvent::mouse(pos)),
            (
                PointerAction::Pressed,
                PointerEvent::mouse(pos).with_button(button),
            ),
            (
                PointerAction::Released,
                PointerEvent::mouse(pos).with_button(button),
            ),
        ] {
            harness.deliver_to_first(SurfaceEvent::Pointer {
                action,
                event,
                modifiers: Modifiers::NONE,
                timestamp: now,
            });
        }
        pump_frames(harness);
    }

    fn send_key(
        harness: &mut Harness<Box<dyn AppHandler>>,
        event: KeyEvent,
        modifiers: Modifiers,
        ts: &Cell<u64>,
    ) {
        let down_ts = Timestamp::from_origin(Duration::from_millis(ts.get()));
        let up_ts = Timestamp::from_origin(Duration::from_millis(ts.get() + 8));
        ts.set(ts.get() + 16);
        harness.deliver_to_first(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: event.clone(),
            modifiers,
            timestamp: down_ts,
        });
        harness.deliver_to_first(SurfaceEvent::Key {
            state: KeyState::Released,
            event,
            modifiers,
            timestamp: up_ts,
        });
        pump_frames(harness);
    }

    fn type_chars(harness: &mut Harness<Box<dyn AppHandler>>, text: &str, ts: &Cell<u64>) {
        for ch in text.chars() {
            let buf = ch.to_string();
            send_key(harness, KeyEvent::character(buf), Modifiers::NONE, ts);
        }
    }

    fn wait_until(
        harness: &mut Harness<Box<dyn AppHandler>>,
        timeout: Duration,
        mut condition: impl FnMut() -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        while !condition() {
            if Instant::now() >= deadline {
                return false;
            }
            harness.advance(Duration::from_millis(25));
            harness.pump();
            std::thread::sleep(Duration::from_millis(10));
        }
        pump_frames(harness);
        true
    }

    fn capture_checkpoint(
        name: &str,
        harness: &mut Harness<Box<dyn AppHandler>>,
        semantic: &mut SemanticStore,
        capture: &Rc<RefCell<CaptureState>>,
        artifacts: &Path,
    ) -> Result<()> {
        pump_frames(harness);
        if let Some(offscreen) = harness.platform().offscreens().first() {
            semantic.ingest(offscreen);
        }
        let a11y_path = artifacts.join(format!("{name}.accessibility.txt"));
        fs::write(&a11y_path, semantic.dump())
            .with_context(|| format!("write accessibility dump {}", a11y_path.display()))?;
        let img = capture
            .borrow()
            .latest_rgba
            .clone()
            .context("expected CaptureRenderer frame before checkpoint capture")?;
        let png_path = artifacts.join(format!("{name}.png"));
        img.save_with_format(&png_path, image::ImageFormat::Png)
            .with_context(|| format!("write screenshot {}", png_path.display()))?;
        Ok(())
    }

    fn prepare_artifacts() -> Result<(PathBuf, PathBuf)> {
        let artifacts = match std::env::var_os("GDS_E2E_ARTIFACTS") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => {
                let dir =
                    std::env::temp_dir().join(format!("gds-files-e2e-{}", std::process::id()));
                fs::create_dir_all(&dir)?;
                dir
            }
        };
        let work = std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("work"))
            .filter(|dir| dir.is_dir())
            .unwrap_or_else(|| artifacts.join("work"));
        fs::create_dir_all(&work)?;
        if !work.join("original.txt").exists() {
            fs::write(work.join("original.txt"), b"original payload\n")?;
        }
        if !work.join("notes.txt").exists() {
            fs::write(work.join("notes.txt"), b"notes payload\n")?;
        }
        let sub = work.join("folder_a");
        fs::create_dir_all(&sub)?;
        if !sub.join("sub.txt").exists() {
            fs::write(sub.join("sub.txt"), b"sub payload\n")?;
        }
        Ok((artifacts, work))
    }

    pub(super) fn execute() -> Result<()> {
        let (artifacts, work) = prepare_artifacts()?;
        let home = work.parent().unwrap_or(&artifacts).to_path_buf();

        let supervisor = Supervisor::new(false).context("start Files E2E Tokio task supervisor")?;
        let scope = supervisor.scope();
        let capture_state = Rc::new(RefCell::new(CaptureState::default()));
        let capture_for_renderer = Rc::clone(&capture_state);
        let browser_slot: Rc<RefCell<Option<Browser>>> = Rc::new(RefCell::new(None));
        let browser_for_view = Rc::clone(&browser_slot);
        let start_dir = work.clone();
        let home_dir = home.clone();

        let handler = app()
            .with_title("Files")
            .with_application_id("dev.gds.Files")
            .with_size(WINDOW_WIDTH, WINDOW_HEIGHT)
            .with_renderer(Box::new(move |_surface, target| {
                let mut renderer = CaptureRenderer::with_state(Rc::clone(&capture_for_renderer));
                renderer.configure(target);
                Ok::<_, zgui::Error>(Box::new(renderer) as Box<dyn Renderer>)
            }))
            .with_context(move || {
                let gds = Gds::install_with_logs(scope.clone(), None);
                let prefs = Preferences {
                    animations_enabled: Some(false),
                    ..Preferences::default()
                };
                let _ = gds.appearance().theme().apply(&prefs.appearance);
                if let Ok(catalog) = files_i18n::prepare(Language::English) {
                    gds.i18n().install_catalog(catalog);
                }
                let _ = PreferencesService::install(prefs, &scope);
            })
            .into_handler(move || {
                let _ = Gds::current().appearance().install_window_sheet();
                let root_scope = Gds::current().tasks().scope();
                let volumes = volumes::Service::new(root_scope);
                let places = Places::default();
                let browser = Browser::new(
                    LaunchOptions {
                        places: Ok(places),
                        start: start_dir.clone(),
                        home: home_dir.clone(),
                        picker: None,
                    },
                    volumes,
                    Rc::new(|_| {}),
                );
                *browser_for_view.borrow_mut() = Some(browser.clone());
                browser.view()
            })
            .context("build headless Files application")?;

        let scenario_error: Rc<RefCell<Option<anyhow::Error>>> = Rc::new(RefCell::new(None));
        let scenario_error_out = Rc::clone(&scenario_error);

        handler
            .drive(move |app| -> Result<(), PlatformError> {
                let mut harness = Harness::new(app);
                if let Some(offscreen) = harness.platform().offscreens().first() {
                    offscreen.set_a11y_listener(true);
                }
                harness.deliver_to_first(SurfaceEvent::Focused(true));
                pump_frames(&mut harness);

                let result = (|| -> Result<()> {
                    let browser = browser_slot
                        .borrow()
                        .clone()
                        .context("Files Browser was not constructed")?;
                    let ts = Cell::new(100_u64);
                    let mut semantic = SemanticStore::default();

                    // 01: Open seeded workspace & verify rendered frame + semantic dump.
                    if !wait_until(&mut harness, Duration::from_secs(3), || {
                        capture_state.borrow().frames > 0
                    }) {
                        bail!("first frame never rendered");
                    }
                    capture_checkpoint(
                        "01-open",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // 02: Rename + Undo + Redo.
                    if let Some(pos) = semantic.find_center_containing("original.txt", None) {
                        send_click(&mut harness, pos, PointerButton::Primary, &ts);
                    } else {
                        browser.select_row(0, Modifiers::NONE);
                    }
                    send_key(
                        &mut harness,
                        KeyEvent::named(NamedKey::F2, PhysicalKey::Code(KeyCode::F2)),
                        Modifiers::NONE,
                        &ts,
                    );
                    type_chars(&mut harness, "renamed.txt", &ts);
                    send_key(
                        &mut harness,
                        KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter)),
                        Modifiers::NONE,
                        &ts,
                    );
                    let _ = wait_until(&mut harness, Duration::from_secs(2), || {
                        work.join("renamed.txt").exists()
                    });
                    send_key(
                        &mut harness,
                        KeyEvent::character("z"),
                        Modifiers::CONTROL,
                        &ts,
                    );
                    pump_frames(&mut harness);
                    send_key(
                        &mut harness,
                        KeyEvent::character("y"),
                        Modifiers::CONTROL,
                        &ts,
                    );
                    capture_checkpoint(
                        "02-rename-undo-redo",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // 03: New folder shortcut (Ctrl+Shift+N).
                    send_key(
                        &mut harness,
                        KeyEvent::character("n"),
                        Modifiers::CONTROL | Modifiers::SHIFT,
                        &ts,
                    );
                    type_chars(&mut harness, "created-dir", &ts);
                    send_key(
                        &mut harness,
                        KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter)),
                        Modifiers::NONE,
                        &ts,
                    );
                    let _ = wait_until(&mut harness, Duration::from_secs(2), || {
                        work.join("created-dir").is_dir()
                    });
                    capture_checkpoint(
                        "03-new-folder",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // 04: Trash & restore flow.
                    if let Some(pos) = semantic.find_center_containing("created-dir", None) {
                        send_click(&mut harness, pos, PointerButton::Primary, &ts);
                    }
                    send_key(
                        &mut harness,
                        KeyEvent::named(NamedKey::Delete, PhysicalKey::Code(KeyCode::Delete)),
                        Modifiers::NONE,
                        &ts,
                    );
                    pump_frames(&mut harness);
                    browser.open(Location::Trash);
                    pump_frames(&mut harness);
                    capture_checkpoint(
                        "04-trash-restore",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // 05: External filesystem change watch.
                    browser.open(Location::from(&work));
                    pump_frames(&mut harness);
                    fs::write(work.join("external-added.txt"), b"watched payload\n")?;
                    let _ = wait_until(&mut harness, Duration::from_secs(2), || {
                        browser
                            .0
                            .state
                            .borrow()
                            .tab()
                            .rows
                            .iter()
                            .any(|row| row.label() == "external-added.txt")
                    });
                    capture_checkpoint(
                        "05-external-watch",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // 06: Grid view switch.
                    browser.toggle_view();
                    pump_frames(&mut harness);
                    let _ = ViewMode::Grid;
                    capture_checkpoint(
                        "06-grid-switch",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // 07: Tabs + pointer drag sequence across tab strip.
                    send_key(
                        &mut harness,
                        KeyEvent::character("n"),
                        Modifiers::CONTROL,
                        &ts,
                    );
                    browser.new_tab(Some(Location::from(work.join("folder_a"))));
                    pump_frames(&mut harness);
                    let drag_start = Point::new(CssPx(280.0), CssPx(52.0));
                    let drag_end = Point::new(CssPx(420.0), CssPx(52.0));
                    harness.deliver_to_first(SurfaceEvent::Pointer {
                        action: PointerAction::Pressed,
                        event: PointerEvent::mouse(drag_start).with_button(PointerButton::Primary),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::from_origin(Duration::from_millis(ts.get())),
                    });
                    harness.deliver_to_first(SurfaceEvent::Pointer {
                        action: PointerAction::Moved,
                        event: PointerEvent::mouse(drag_end),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::from_origin(Duration::from_millis(ts.get() + 16)),
                    });
                    harness.deliver_to_first(SurfaceEvent::Pointer {
                        action: PointerAction::Released,
                        event: PointerEvent::mouse(drag_end).with_button(PointerButton::Primary),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::from_origin(Duration::from_millis(ts.get() + 32)),
                    });
                    ts.set(ts.get() + 48);
                    capture_checkpoint(
                        "07-tab-drag",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // 08: Filter + IME composition & commit.
                    send_key(
                        &mut harness,
                        KeyEvent::character("f"),
                        Modifiers::CONTROL,
                        &ts,
                    );
                    harness.deliver_to_first(SurfaceEvent::Ime(ImeEvent::Enabled));
                    harness.deliver_to_first(SurfaceEvent::Ime(ImeEvent::Preedit {
                        text: "bi".into(),
                        cursor: Some(0..2),
                    }));
                    harness.deliver_to_first(SurfaceEvent::Ime(ImeEvent::Commit("notes".into())));
                    harness.deliver_to_first(SurfaceEvent::Ime(ImeEvent::Disabled));
                    pump_frames(&mut harness);
                    capture_checkpoint(
                        "08-search-ime",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;
                    send_key(
                        &mut harness,
                        KeyEvent::named(NamedKey::Escape, PhysicalKey::Code(KeyCode::Escape)),
                        Modifiers::NONE,
                        &ts,
                    );

                    // 09: Details panel + embedded terminal dock.
                    browser.toggle_details();
                    browser.toggle_terminal();
                    pump_frames(&mut harness);
                    capture_checkpoint(
                        "09-details-terminal",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // 10: Hierarchical tag address + mouse Back/Forward navigation.
                    if let Ok(tag_root) = TagAddress::new("work") {
                        browser.open(Location::Tags(tag_root));
                    }
                    if let Ok(tag_child) = TagAddress::new("work/urgent") {
                        browser.open(Location::Tags(tag_child));
                    }
                    browser.open(Location::from(&work));
                    pump_frames(&mut harness);
                    send_click(
                        &mut harness,
                        Point::new(CssPx(640.0), CssPx(450.0)),
                        PointerButton::Back,
                        &ts,
                    );
                    send_click(
                        &mut harness,
                        Point::new(CssPx(640.0), CssPx(450.0)),
                        PointerButton::Forward,
                        &ts,
                    );
                    capture_checkpoint(
                        "10-location-mouse-history",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    let frames = capture_state.borrow().frames;
                    let summary = format!(
                        "PASS driven\nframes={frames}\nartifacts={}\n",
                        artifacts.display()
                    );
                    fs::write(artifacts.join("driven-passed.txt"), summary)?;
                    Ok(())
                })();

                if let Err(err) = result {
                    *scenario_error_out.borrow_mut() = Some(err);
                }
                harness.shut_down();
                Ok(())
            })
            .map_err(|error| anyhow::anyhow!("zgui harness failed: {error}"))?;

        if let Some(err) = scenario_error.borrow_mut().take() {
            return Err(err);
        }
        Ok(())
    }
}
