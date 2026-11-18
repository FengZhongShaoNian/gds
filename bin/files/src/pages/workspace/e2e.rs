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
    use std::collections::{HashMap, HashSet};
    use std::fmt::Write as _;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, ensure};
    use gds::app::Gds;
    use gds::i18n::Language;
    use gds::service::task_manager::Supervisor;
    use zgui::app::app;
    use zgui::atlas::{MemorySink, TextureId, TextureKind, TextureSink};
    use zgui::bits::DamageSet;
    use zgui::geom::{Css, CssPx, DevicePx, Matrix4, Point, Rect, Size, transformed_bounds};
    use zgui::platform::{AppHandler, PlatformError, SurfaceEvent};
    use zgui::prelude::GetUntracked;
    use zgui::render::{
        ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
        Renderer, TextureHandle,
    };
    use zgui::scene::{ClipId, ColorSprite, Paint, PaintId, PaintKind, Placements, Scene};
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
    const CLEAR_PIXEL: image::Rgba<u8> = image::Rgba([24, 24, 28, 255]);

    #[derive(Default)]
    struct CaptureState {
        latest_rgba: Option<image::RgbaImage>,
        frames: u64,
    }

    #[derive(Default)]
    struct CaptureRenderer {
        target: Option<RenderTarget>,
        atlas: MemorySink,
        canvas: Option<image::RgbaImage>,
        capture: Rc<RefCell<CaptureState>>,
    }

    impl CaptureRenderer {
        fn with_state(capture: Rc<RefCell<CaptureState>>) -> Self {
            Self {
                target: None,
                atlas: MemorySink::new(),
                canvas: None,
                capture,
            }
        }
    }

    struct DamageRegion {
        full: bool,
        rects: Vec<[i32; 4]>,
        bbox: [i32; 4],
    }

    impl DamageRegion {
        fn full(max_w: i32, max_h: i32) -> Self {
            let bbox = [0, 0, max_w, max_h];
            Self {
                full: true,
                rects: vec![bbox],
                bbox,
            }
        }

        fn from_rects(rects: Vec<[i32; 4]>) -> Option<Self> {
            if rects.is_empty() {
                return None;
            }
            let mut bbox = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
            for &[x0, y0, x1, y1] in &rects {
                bbox[0] = bbox[0].min(x0);
                bbox[1] = bbox[1].min(y0);
                bbox[2] = bbox[2].max(x1);
                bbox[3] = bbox[3].max(y1);
            }
            Some(Self {
                full: false,
                rects,
                bbox,
            })
        }

        fn contains(&self, x: i32, y: i32) -> bool {
            self.full
                || self.rects.len() == 1
                || self
                    .rects
                    .iter()
                    .any(|&[x0, y0, x1, y1]| x >= x0 && x < x1 && y >= y0 && y < y1)
        }

        fn clipped_span(&self, bounds: [f32; 4], clip: [i32; 4]) -> Option<[i32; 4]> {
            if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
                return None;
            }
            let x0 = (bounds[0].floor() as i32).max(clip[0]).max(self.bbox[0]);
            let y0 = (bounds[1].floor() as i32).max(clip[1]).max(self.bbox[1]);
            let x1 = ((bounds[0] + bounds[2]).ceil() as i32)
                .min(clip[2])
                .min(self.bbox[2]);
            let y1 = ((bounds[1] + bounds[3]).ceil() as i32)
                .min(clip[3])
                .min(self.bbox[3]);
            (x0 < x1 && y0 < y1).then_some([x0, y0, x1, y1])
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

    fn fill_rect(
        img: &mut image::RgbaImage,
        bounds: [f32; 4],
        clip: [i32; 4],
        damage: &DamageRegion,
        color: [f32; 4],
    ) {
        if color[3] <= 0.0 {
            return;
        }
        let Some([x0, y0, x1, y1]) = damage.clipped_span(bounds, clip) else {
            return;
        };
        for y in y0..y1 {
            for x in x0..x1 {
                if !damage.contains(x, y) {
                    continue;
                }
                let px = img.get_pixel_mut(x as u32, y as u32);
                blend_premultiplied(&mut px.0, color);
            }
        }
    }

    fn place_bounds(matrices: &[Matrix4], slot: u32, bounds: [f32; 4]) -> [f32; 4] {
        if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
            return [0.0; 4];
        }
        let Some(matrix) = matrices.get(slot as usize) else {
            return bounds;
        };
        let rect = Rect::new(
            Point::new(DevicePx(bounds[0]), DevicePx(bounds[1])),
            Size::new(DevicePx(bounds[2]), DevicePx(bounds[3])),
        );
        let placed = transformed_bounds(matrix, rect);
        [
            placed.origin.x.0,
            placed.origin.y.0,
            placed.size.width.0,
            placed.size.height.0,
        ]
    }

    fn resolve_clip_rect(
        scene: &Scene,
        placements: &Placements,
        clip: u32,
        max_w: i32,
        max_h: i32,
    ) -> [i32; 4] {
        let resolved = scene
            .clips
            .resolve_placed(ClipId(clip), &|space| placements.get(space).copied());
        let cx0 = f64::from(resolved.aabb[0])
            .floor()
            .clamp(0.0, f64::from(max_w)) as i32;
        let cy0 = f64::from(resolved.aabb[1])
            .floor()
            .clamp(0.0, f64::from(max_h)) as i32;
        let cx1 = (f64::from(resolved.aabb[0]) + f64::from(resolved.aabb[2]))
            .ceil()
            .clamp(0.0, f64::from(max_w)) as i32;
        let cy1 = (f64::from(resolved.aabb[1]) + f64::from(resolved.aabb[3]))
            .ceil()
            .clamp(0.0, f64::from(max_h)) as i32;
        [cx0, cy0, cx1, cy1]
    }

    fn intersect_clip(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
        [
            a[0].max(b[0]),
            a[1].max(b[1]),
            a[2].min(b[2]),
            a[3].min(b[3]),
        ]
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

    fn paint_color(scene: &Scene, kind: u32, index: u32) -> Option<[f32; 4]> {
        if kind == PaintKind::None as u32 {
            return None;
        }
        match scene.paints.get(PaintId(index)) {
            Some(Paint::Solid(color)) => Some(color.to_premultiplied_srgb()),
            Some(Paint::Gradient { stops, .. }) => {
                stops.first().map(|stop| stop.color.to_premultiplied_srgb())
            }
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

        fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
            let size = self
                .target
                .map(|target| target.size)
                .unwrap_or_else(|| scene.viewport());
            let width = u32::try_from(size.width.max(1)).unwrap_or(1);
            let height = u32::try_from(size.height.max(1)).unwrap_or(1);
            let max_w = i32::try_from(width).unwrap_or(i32::MAX);
            let max_h = i32::try_from(height).unwrap_or(i32::MAX);

            let resized = self
                .canvas
                .as_ref()
                .is_none_or(|canvas| canvas.width() != width || canvas.height() != height);
            if resized {
                self.canvas = Some(image::RgbaImage::from_pixel(width, height, CLEAR_PIXEL));
            }
            let img = self
                .canvas
                .get_or_insert_with(|| image::RgbaImage::from_pixel(width, height, CLEAR_PIXEL));

            let full_clear = resized || damage.is_full();
            let region = if full_clear {
                for px in img.pixels_mut() {
                    *px = CLEAR_PIXEL;
                }
                DamageRegion::full(max_w, max_h)
            } else if damage.is_empty() {
                let mut guard = self.capture.borrow_mut();
                guard.latest_rgba = Some(img.clone());
                guard.frames = guard.frames.saturating_add(1);
                return FrameOutcome::Presented(FrameStats {
                    vector_passes: 0,
                    draw_calls: 0,
                    damage_px: 0,
                    bytes_uploaded: 0,
                    memory: MemoryReport::ZERO,
                });
            } else {
                let rects: Vec<[i32; 4]> = damage
                    .rects()
                    .iter()
                    .filter_map(|r| {
                        let x0 = r.origin.x.clamp(0, max_w);
                        let y0 = r.origin.y.clamp(0, max_h);
                        let x1 = r.origin.x.saturating_add(r.size.width).clamp(0, max_w);
                        let y1 = r.origin.y.saturating_add(r.size.height).clamp(0, max_h);
                        (x0 < x1 && y0 < y1).then_some([x0, y0, x1, y1])
                    })
                    .collect();
                let Some(region) = DamageRegion::from_rects(rects) else {
                    let mut guard = self.capture.borrow_mut();
                    guard.latest_rgba = Some(img.clone());
                    guard.frames = guard.frames.saturating_add(1);
                    return FrameOutcome::Presented(FrameStats {
                        vector_passes: 0,
                        draw_calls: 0,
                        damage_px: 0,
                        bytes_uploaded: 0,
                        memory: MemoryReport::ZERO,
                    });
                };
                for &[x0, y0, x1, y1] in &region.rects {
                    for py in y0..y1 {
                        for px in x0..x1 {
                            img.put_pixel(px as u32, py as u32, CLEAR_PIXEL);
                        }
                    }
                }
                region
            };

            let placements = scene.placements();
            let matrices: Vec<Matrix4> = placements.matrices().collect();

            #[derive(Clone, Copy)]
            enum DrawItem {
                Quad(usize, u32),
                Mono(usize, u32),
                Color(usize, u32),
            }

            let mut items = Vec::with_capacity(
                scene.primitives.quads.len()
                    + scene.primitives.mono_sprites.len()
                    + scene.primitives.color_sprites.len(),
            );
            for (idx, quad) in scene.primitives.quads.iter().enumerate() {
                items.push(DrawItem::Quad(idx, quad.order));
            }
            for (idx, sprite) in scene.primitives.mono_sprites.iter().enumerate() {
                items.push(DrawItem::Mono(idx, sprite.order));
            }
            for (idx, sprite) in scene.primitives.color_sprites.iter().enumerate() {
                items.push(DrawItem::Color(idx, sprite.order));
            }
            items.sort_by_key(|item| match *item {
                DrawItem::Quad(idx, order) => (order, 0_u8, idx),
                DrawItem::Color(idx, order) => (order, 1_u8, idx),
                DrawItem::Mono(idx, order) => (order, 2_u8, idx),
            });

            for item in items {
                match item {
                    DrawItem::Quad(idx, _) => {
                        let quad = &scene.primitives.quads[idx];
                        let placed = place_bounds(&matrices, quad.transform, quad.bounds);
                        let clip = resolve_clip_rect(scene, &placements, quad.clip, max_w, max_h);
                        if let Some(fill) = paint_color(scene, quad.fill.kind, quad.fill.index) {
                            fill_rect(img, placed, clip, &region, fill);
                        }
                        if let Some(stroke) =
                            paint_color(scene, quad.stroke.kind, quad.stroke.index)
                        {
                            let [x, y, w, h] = placed;
                            let [t, r, b, l] = quad.border;
                            if t > 0.0 {
                                fill_rect(img, [x, y, w, t], clip, &region, stroke);
                            }
                            if b > 0.0 {
                                fill_rect(img, [x, y + h - b, w, b], clip, &region, stroke);
                            }
                            if l > 0.0 {
                                fill_rect(img, [x, y, l, h], clip, &region, stroke);
                            }
                            if r > 0.0 {
                                fill_rect(img, [x + w - r, y, r, h], clip, &region, stroke);
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
                        let placed = place_bounds(&matrices, sprite.transform, sprite.bounds);
                        let [dx, dy, dw, dh] = placed;
                        if tw <= 0 || th <= 0 || dw <= 0.0 || dh <= 0.0 {
                            continue;
                        }
                        let clip = resolve_clip_rect(scene, &placements, sprite.clip, max_w, max_h);
                        let Some([start_x, start_y, end_x, end_y]) =
                            region.clipped_span(placed, clip)
                        else {
                            continue;
                        };
                        for py in start_y..end_y {
                            let v = ((py as f32 + 0.5 - dy) / dh) * th as f32;
                            let sy = ty + (v.floor() as i32).clamp(0, th - 1);
                            for px in start_x..end_x {
                                if !region.contains(px, py) {
                                    continue;
                                }
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
                    DrawItem::Color(idx, _) => {
                        let sprite = &scene.primitives.color_sprites[idx];
                        if sprite.tile.is_unresolved() {
                            continue;
                        }
                        let Some(tex_id) = texture_id_from_word(sprite.tile.texture) else {
                            continue;
                        };
                        let [tx, ty, tw, th] = sprite.tile.bounds;
                        let placed = place_bounds(&matrices, sprite.transform, sprite.bounds);
                        let [dx, dy, dw, dh] = placed;
                        let opacity = sprite.opacity.clamp(0.0, 1.0);
                        if tw <= 0 || th <= 0 || dw <= 0.0 || dh <= 0.0 || opacity <= 0.0 {
                            continue;
                        }
                        let frame = place_bounds(&matrices, sprite.transform, sprite.frame);
                        let frame_clip = [
                            frame[0].floor().clamp(0.0, max_w as f32) as i32,
                            frame[1].floor().clamp(0.0, max_h as f32) as i32,
                            (frame[0] + frame[2]).ceil().clamp(0.0, max_w as f32) as i32,
                            (frame[1] + frame[3]).ceil().clamp(0.0, max_h as f32) as i32,
                        ];
                        let scene_clip =
                            resolve_clip_rect(scene, &placements, sprite.clip, max_w, max_h);
                        let clip = intersect_clip(scene_clip, frame_clip);
                        let Some([start_x, start_y, end_x, end_y]) =
                            region.clipped_span(placed, clip)
                        else {
                            continue;
                        };
                        let grayscale = (sprite.flags & ColorSprite::GRAYSCALE) != 0;
                        for py in start_y..end_y {
                            let v = ((py as f32 + 0.5 - dy) / dh) * th as f32;
                            let sy = ty + (v.floor() as i32).clamp(0, th - 1);
                            for px in start_x..end_x {
                                if !region.contains(px, py) {
                                    continue;
                                }
                                let u = ((px as f32 + 0.5 - dx) / dw) * tw as f32;
                                let sx = tx + (u.floor() as i32).clamp(0, tw - 1);
                                if let Some(texel) = self.atlas.texel(tex_id, sx, sy)
                                    && texel.len() >= 4
                                    && texel[3] > 0
                                {
                                    let mut r = (f32::from(texel[0]) / 255.0) * opacity;
                                    let mut g = (f32::from(texel[1]) / 255.0) * opacity;
                                    let mut b = (f32::from(texel[2]) / 255.0) * opacity;
                                    let a = (f32::from(texel[3]) / 255.0) * opacity;
                                    if grayscale {
                                        let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                                        r = lum;
                                        g = lum;
                                        b = lum;
                                    }
                                    let px_dst = img.get_pixel_mut(px as u32, py as u32);
                                    blend_premultiplied(&mut px_dst.0, [r, g, b, a]);
                                }
                            }
                        }
                    }
                }
            }

            {
                let mut guard = self.capture.borrow_mut();
                guard.latest_rgba = Some(img.clone());
                guard.frames = guard.frames.saturating_add(1);
            }

            let damage_px = if full_clear {
                u64::from(width) * u64::from(height)
            } else {
                region
                    .rects
                    .iter()
                    .map(|&[x0, y0, x1, y1]| {
                        u64::try_from((x1 - x0).max(0)).unwrap_or(0)
                            * u64::try_from((y1 - y0).max(0)).unwrap_or(0)
                    })
                    .sum()
            };

            FrameOutcome::Presented(FrameStats {
                vector_passes: 0,
                draw_calls: u32::try_from(
                    scene.primitives.quads.len()
                        + scene.primitives.mono_sprites.len()
                        + scene.primitives.color_sprites.len(),
                )
                .unwrap_or(u32::MAX),
                damage_px,
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
        transform: Option<[f64; 6]>,
        children: Vec<u64>,
        disabled: bool,
    }

    struct ResolvedNode {
        id: u64,
        depth: usize,
        role: Role,
        label: Option<String>,
        value: Option<String>,
        bounds: Option<(f64, f64, f64, f64)>,
        disabled: bool,
    }

    const IDENTITY_AFFINE: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    fn compose_affine(outer: [f64; 6], inner: [f64; 6]) -> [f64; 6] {
        [
            outer[0] * inner[0] + outer[2] * inner[1],
            outer[1] * inner[0] + outer[3] * inner[1],
            outer[0] * inner[2] + outer[2] * inner[3],
            outer[1] * inner[2] + outer[3] * inner[3],
            outer[0] * inner[4] + outer[2] * inner[5] + outer[4],
            outer[1] * inner[4] + outer[3] * inner[5] + outer[5],
        ]
    }

    fn apply_affine(m: [f64; 6], x: f64, y: f64) -> (f64, f64) {
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    fn transform_a11y_rect(m: [f64; 6], r: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
        let corners = [
            apply_affine(m, r.0, r.1),
            apply_affine(m, r.2, r.1),
            apply_affine(m, r.2, r.3),
            apply_affine(m, r.0, r.3),
        ];
        let min_x = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
        let min_y = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
        let max_x = corners
            .iter()
            .map(|c| c.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = corners
            .iter()
            .map(|c| c.1)
            .fold(f64::NEG_INFINITY, f64::max);
        (min_x, min_y, max_x, max_y)
    }

    #[derive(Default)]
    struct SemanticStore {
        root: Option<u64>,
        focus: Option<u64>,
        nodes: HashMap<u64, NodeEntry>,
    }

    impl SemanticStore {
        fn ingest_from(&mut self, harness: &Harness<Box<dyn AppHandler>>) {
            if let Some(offscreen) = harness.platform().offscreens().first() {
                self.ingest(offscreen);
            }
        }

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
                    let transform = node.transform().map(|t| t.as_coeffs());
                    let children = node.children().iter().map(|child| child.0).collect();
                    self.nodes.insert(
                        id.0,
                        NodeEntry {
                            role: node.role(),
                            label: node.label().map(str::to_owned),
                            value: node.value().map(str::to_owned),
                            bounds,
                            transform,
                            children,
                            disabled: node.is_disabled(),
                        },
                    );
                }
            }
        }

        fn resolved_live_nodes(&self) -> Vec<ResolvedNode> {
            let mut out = Vec::new();
            let Some(root) = self.root else {
                return out;
            };
            let mut visited = HashSet::new();
            self.collect_live(root, 0, IDENTITY_AFFINE, &mut visited, &mut out);
            out
        }

        fn collect_live(
            &self,
            id: u64,
            depth: usize,
            world: [f64; 6],
            visited: &mut HashSet<u64>,
            out: &mut Vec<ResolvedNode>,
        ) {
            if !visited.insert(id) {
                return;
            }
            let Some(node) = self.nodes.get(&id) else {
                return;
            };
            let local_to_world = match node.transform {
                Some(t) => compose_affine(world, t),
                None => world,
            };
            let resolved_bounds = node.bounds.map(|b| transform_a11y_rect(local_to_world, b));
            out.push(ResolvedNode {
                id,
                depth,
                role: node.role,
                label: node.label.clone(),
                value: node.value.clone(),
                bounds: resolved_bounds,
                disabled: node.disabled,
            });
            for &child in &node.children {
                self.collect_live(child, depth + 1, local_to_world, visited, out);
            }
        }

        fn dump(&self) -> String {
            let live = self.resolved_live_nodes();
            if live.is_empty() {
                return String::from("(empty accessibility tree)\n");
            }
            let mut out = String::new();
            let _ = writeln!(out, "focus={:?}", self.focus);
            for node in live {
                let indent = "  ".repeat(node.depth);
                let _ = write!(out, "{indent}- {:?} (id={})", node.role, node.id);
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
            }
            out
        }

        fn has_role(&self, expected: Role) -> bool {
            self.resolved_live_nodes()
                .into_iter()
                .any(|node| node.role == expected)
        }

        fn dialog_height(&self) -> Option<f64> {
            self.resolved_live_nodes().into_iter().find_map(|node| {
                if node.role != Role::Dialog {
                    return None;
                }
                let (_x0, y0, _x1, y1) = node.bounds?;
                Some(y1 - y0)
            })
        }

        fn find_entry_center(&self, needle: &str) -> Option<Point<CssPx, Css>> {
            self.resolved_live_nodes().into_iter().find_map(|node| {
                if !matches!(node.role, Role::Label | Role::ListItem) {
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
                if x0 < 220.0 || y0 < 75.0 || x1 <= x0 || y1 <= y0 {
                    return None;
                }
                Some(Point::new(
                    CssPx(((x0 + x1) * 0.5) as f32),
                    CssPx(((y0 + y1) * 0.5) as f32),
                ))
            })
        }

        fn find_sidebar_button(
            &self,
            needle: &str,
            max_right: f64,
        ) -> Option<(Point<CssPx, Css>, f64)> {
            self.resolved_live_nodes().into_iter().find_map(|node| {
                if node.role != Role::Button {
                    return None;
                }
                let matches_label = node
                    .label
                    .as_deref()
                    .is_some_and(|label| label.contains(needle));
                if !matches_label {
                    return None;
                }
                let (x0, y0, x1, y1) = node.bounds?;
                if x0 >= max_right || y0 < 40.0 {
                    return None;
                }
                Some((
                    Point::new(
                        CssPx(((x0 + x1) * 0.5) as f32),
                        CssPx(((y0 + y1) * 0.5) as f32),
                    ),
                    x1 - x0,
                ))
            })
        }

        fn assert_sidebar_buttons_fit(&self, sidebar_width: f64) -> Result<()> {
            for node in self.resolved_live_nodes() {
                if node.role == Role::Button
                    && let Some((x0, y0, x1, _y1)) = node.bounds
                    && x0 < sidebar_width
                    && y0 >= 40.0
                {
                    let width = x1 - x0;
                    ensure!(
                        width <= sidebar_width + 0.5 && x1 <= sidebar_width + 0.5,
                        "sidebar button {:?} overflowed sidebar ({x0:.1}..{x1:.1}, width={width:.1} > {sidebar_width:.1})",
                        node.label
                    );
                }
            }
            Ok(())
        }
    }

    fn pump_frames(harness: &mut Harness<Box<dyn AppHandler>>) {
        for _ in 0..8 {
            harness.pump();
            harness.advance(Duration::from_millis(30));
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
            harness.pump();
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
        mut condition: impl FnMut(&Harness<Box<dyn AppHandler>>) -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        while !condition(harness) {
            if Instant::now() >= deadline {
                return false;
            }
            harness.advance(Duration::from_millis(30));
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
        semantic.ingest_from(harness);
        ensure!(
            !semantic.has_role(Role::Dialog),
            "checkpoint {name}: unexpected open Role::Dialog remained mounted"
        );
        let a11y_path = artifacts.join(format!("{name}.accessibility.txt"));
        fs::write(&a11y_path, semantic.dump())
            .with_context(|| format!("write accessibility dump {}", a11y_path.display()))?;
        let img = capture
            .borrow()
            .latest_rgba
            .clone()
            .context("expected CaptureRenderer frame before checkpoint capture")?;
        let first = *img.get_pixel(0, 0);
        ensure!(
            img.pixels().any(|px| *px != first),
            "checkpoint {name}: rendered frame is uniform solid color"
        );
        let png_path = artifacts.join(format!("{name}.png"));
        img.save_with_format(&png_path, image::ImageFormat::Png)
            .with_context(|| format!("write screenshot {}", png_path.display()))?;
        Ok(())
    }

    fn prepare_artifacts() -> Result<(PathBuf, PathBuf)> {
        let artifacts = match std::env::var_os("GDS_E2E_ARTIFACTS") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => std::env::temp_dir().join(format!("gds-files-e2e-{}", std::process::id())),
        };
        fs::create_dir_all(&artifacts)?;
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
            .with_application_id("io.github.GanDesktop.GdsFiles")
            .with_size(WINDOW_WIDTH, WINDOW_HEIGHT)
            .with_renderer(Box::new(move |_surface, target| {
                let mut renderer = CaptureRenderer::with_state(Rc::clone(&capture_for_renderer));
                renderer.configure(target);
                Ok::<_, zgui::Error>(Box::new(renderer) as Box<dyn Renderer>)
            }))
            .with_context(move || {
                let gds = Gds::install_with_logs(scope.clone(), None);
                gds.appearance().motion().set_enabled(false);
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
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(3), |h| {
                            semantic.ingest_from(h);
                            capture_state.borrow().frames > 0
                                && semantic.find_entry_center("original.txt").is_some()
                        }),
                        "01: initial workspace listing did not render"
                    );
                    capture_checkpoint(
                        "01-open",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;
                    semantic.assert_sidebar_buttons_fit(222.0)?;

                    // 02: Rename + Undo + Redo.
                    let orig_pos = semantic
                        .find_entry_center("original.txt")
                        .context("02: original.txt row missing")?;
                    send_click(&mut harness, orig_pos, PointerButton::Primary, &ts);
                    send_key(
                        &mut harness,
                        KeyEvent::named(NamedKey::F2, PhysicalKey::Code(KeyCode::F2)),
                        Modifiers::NONE,
                        &ts,
                    );
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |h| {
                            semantic.ingest_from(h);
                            semantic.has_role(Role::Dialog)
                        }),
                        "02: rename dialog did not open"
                    );
                    send_key(
                        &mut harness,
                        KeyEvent::character("a"),
                        Modifiers::CONTROL,
                        &ts,
                    );
                    type_chars(&mut harness, "renamed.txt", &ts);
                    send_key(
                        &mut harness,
                        KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter)),
                        Modifiers::NONE,
                        &ts,
                    );
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(3), |_| {
                            work.join("renamed.txt").exists() && !work.join("original.txt").exists()
                        }),
                        "02: rename failed to produce renamed.txt"
                    );
                    pump_frames(&mut harness);
                    semantic.ingest_from(&harness);
                    ensure!(
                        !semantic.has_role(Role::Dialog),
                        "02: rename dialog stayed open after submit"
                    );
                    send_key(
                        &mut harness,
                        KeyEvent::character("z"),
                        Modifiers::CONTROL,
                        &ts,
                    );
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(3), |_| {
                            work.join("original.txt").exists() && !work.join("renamed.txt").exists()
                        }),
                        "02: Ctrl+Z undo failed to restore original.txt"
                    );
                    send_key(
                        &mut harness,
                        KeyEvent::character("z"),
                        Modifiers::CONTROL | Modifiers::SHIFT,
                        &ts,
                    );
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(3), |_| {
                            work.join("renamed.txt").exists() && !work.join("original.txt").exists()
                        }),
                        "02: Ctrl+Shift+Z redo failed to re-apply renamed.txt"
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
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |h| {
                            semantic.ingest_from(h);
                            semantic.has_role(Role::Dialog)
                        }),
                        "03: new folder dialog did not open"
                    );
                    send_key(
                        &mut harness,
                        KeyEvent::character("a"),
                        Modifiers::CONTROL,
                        &ts,
                    );
                    type_chars(&mut harness, "created-dir", &ts);
                    send_key(
                        &mut harness,
                        KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter)),
                        Modifiers::NONE,
                        &ts,
                    );
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(3), |_| {
                            work.join("created-dir").is_dir()
                        }),
                        "03: new folder created-dir was not created"
                    );
                    pump_frames(&mut harness);
                    semantic.ingest_from(&harness);
                    ensure!(
                        !semantic.has_role(Role::Dialog),
                        "03: new folder dialog stayed open after submit"
                    );
                    capture_checkpoint(
                        "03-new-folder",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // Verify animated Prompt dismissal retains dialog content during exit fade-out
                    // rather than collapsing to an empty [x] shell, then unmounts cleanly.
                    browser.0.owner.with(|| {
                        Gds::current().appearance().motion().set_enabled(true);
                    });
                    browser.show_about();
                    harness.pump();
                    semantic.ingest_from(&harness);
                    let open_dialog_h = semantic
                        .dialog_height()
                        .context("03b: animated About dialog did not mount")?;
                    ensure!(
                        open_dialog_h > 120.0,
                        "03b: expected full About dialog height, got {open_dialog_h:.1}"
                    );
                    browser.cancel_prompt();
                    harness.pump();
                    semantic.ingest_from(&harness);
                    if let Some(leaving_h) = semantic.dialog_height() {
                        ensure!(
                            leaving_h > 120.0,
                            "03b: closing Prompt dialog collapsed to empty shell (height={leaving_h:.1}) during exit fade"
                        );
                    }
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |h| {
                            semantic.ingest_from(h);
                            !semantic.has_role(Role::Dialog)
                        }),
                        "03b: animated Prompt dialog did not unmount after exit transition"
                    );
                    browser.0.owner.with(|| {
                        Gds::current().appearance().motion().set_enabled(false);
                    });
                    pump_frames(&mut harness);

                    // 04: Trash & restore flow.
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |h| {
                            semantic.ingest_from(h);
                            semantic.find_entry_center("created-dir").is_some()
                        }),
                        "04: created-dir row not visible before trashing"
                    );
                    let dir_pos = semantic
                        .find_entry_center("created-dir")
                        .context("04: created-dir row missing")?;
                    send_click(&mut harness, dir_pos, PointerButton::Primary, &ts);
                    send_key(
                        &mut harness,
                        KeyEvent::named(NamedKey::Delete, PhysicalKey::Code(KeyCode::Delete)),
                        Modifiers::NONE,
                        &ts,
                    );
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(3), |_| {
                            !work.join("created-dir").exists()
                        }),
                        "04: created-dir was not moved to Trash"
                    );
                    browser.open(Location::Trash);
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(3), |h| {
                            semantic.ingest_from(h);
                            semantic.find_entry_center("created-dir").is_some()
                        }),
                        "04: created-dir did not appear in Trash listing"
                    );
                    let trash_pos = semantic
                        .find_entry_center("created-dir")
                        .context("04: created-dir row missing in Trash")?;
                    send_click(&mut harness, trash_pos, PointerButton::Primary, &ts);
                    capture_checkpoint(
                        "04-trash-restore",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;
                    send_key(
                        &mut harness,
                        KeyEvent::character("r"),
                        Modifiers::ALT,
                        &ts,
                    );
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(3), |_| {
                            work.join("created-dir").is_dir()
                        }),
                        "04: Alt+R restore from Trash failed to restore created-dir"
                    );

                    // 05: External filesystem change watch.
                    browser.open(Location::from(&work));
                    pump_frames(&mut harness);
                    fs::write(work.join("external-added.txt"), b"watched payload\n")?;
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(3), |h| {
                            semantic.ingest_from(h);
                            browser
                                .0
                                .state
                                .borrow()
                                .tab()
                                .rows
                                .iter()
                                .any(|row| row.label() == "external-added.txt")
                                && semantic.find_entry_center("external-added.txt").is_some()
                        }),
                        "05: filesystem watcher did not add external-added.txt"
                    );
                    capture_checkpoint(
                        "05-external-watch",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // 06: Grid view switch.
                    browser.toggle_view();
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |_| {
                            browser.read_untracked(|state| state.tab().mode == ViewMode::Grid)
                        }),
                        "06: toggle_view did not switch to Grid"
                    );
                    capture_checkpoint(
                        "06-grid-switch",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // 07: Tabs + pointer drag sequence across sidebar tabs.
                    send_key(
                        &mut harness,
                        KeyEvent::character("n"),
                        Modifiers::CONTROL,
                        &ts,
                    );
                    browser.new_tab(Some(Location::from(work.join("folder_a"))));
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |h| {
                            semantic.ingest_from(h);
                            semantic.find_sidebar_button("folder_a", 222.0).is_some()
                                && semantic.find_sidebar_button("work", 222.0).is_some()
                        }),
                        "07: sidebar tabs did not mount"
                    );
                    semantic.assert_sidebar_buttons_fit(222.0)?;
                    let (drag_start, folder_tab_w) = semantic
                        .find_sidebar_button("folder_a", 222.0)
                        .context("07: folder_a sidebar tab missing")?;
                    let (drag_end, work_tab_w) = semantic
                        .find_sidebar_button("work", 222.0)
                        .context("07: work sidebar tab missing")?;
                    ensure!(
                        folder_tab_w <= 222.0 && work_tab_w <= 222.0,
                        "07: tab button widths exceeded sidebar ({folder_tab_w:.1}, {work_tab_w:.1})"
                    );
                    let drag_nudge = Point::new(CssPx(drag_start.x.0), CssPx(drag_start.y.0 - 8.0));
                    let drag_mid = Point::new(
                        CssPx((drag_start.x.0 + drag_end.x.0) * 0.5),
                        CssPx((drag_start.y.0 + drag_end.y.0) * 0.5),
                    );
                    harness.deliver_to_first(SurfaceEvent::Pointer {
                        action: PointerAction::Moved,
                        event: PointerEvent::mouse(drag_start),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::from_origin(Duration::from_millis(ts.get())),
                    });
                    harness.pump();
                    harness.deliver_to_first(SurfaceEvent::Pointer {
                        action: PointerAction::Pressed,
                        event: PointerEvent::mouse(drag_start)
                            .with_button(PointerButton::Primary),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::from_origin(Duration::from_millis(ts.get() + 16)),
                    });
                    harness.pump();
                    harness.deliver_to_first(SurfaceEvent::Pointer {
                        action: PointerAction::Moved,
                        event: PointerEvent::mouse(drag_nudge),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::from_origin(Duration::from_millis(ts.get() + 24)),
                    });
                    harness.pump();
                    harness.deliver_to_first(SurfaceEvent::Pointer {
                        action: PointerAction::Moved,
                        event: PointerEvent::mouse(drag_mid),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::from_origin(Duration::from_millis(ts.get() + 32)),
                    });
                    harness.pump();
                    harness.deliver_to_first(SurfaceEvent::Pointer {
                        action: PointerAction::Moved,
                        event: PointerEvent::mouse(drag_end),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::from_origin(Duration::from_millis(ts.get() + 40)),
                    });
                    harness.pump();
                    harness.deliver_to_first(SurfaceEvent::Pointer {
                        action: PointerAction::Released,
                        event: PointerEvent::mouse(drag_end).with_button(PointerButton::Primary),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::from_origin(Duration::from_millis(ts.get() + 48)),
                    });
                    ts.set(ts.get() + 64);
                    pump_frames(&mut harness);
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |_| {
                            browser.read_untracked(|state| {
                                state.tabs.len() == 3
                                    && state
                                        .tabs
                                        .first()
                                        .and_then(|tab| tab.navigation.location().local_path())
                                        .is_some_and(|path| path.ends_with("folder_a"))
                            })
                        }),
                        "07: dragging folder_a sidebar tab onto work tab did not reorder tabs"
                    );
                    capture_checkpoint(
                        "07-tab-drag",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;
                    semantic.assert_sidebar_buttons_fit(222.0)?;

                    // 08: Filter + IME composition & commit.
                    harness.deliver_to_first(SurfaceEvent::Pointer {
                        action: PointerAction::Moved,
                        event: PointerEvent::mouse(Point::new(CssPx(640.0), CssPx(220.0))),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::from_origin(Duration::from_millis(ts.get())),
                    });
                    ts.set(ts.get() + 16);
                    pump_frames(&mut harness);
                    semantic.ingest_from(&harness);
                    let (work_tab_pos, _) = semantic
                        .find_sidebar_button("work", 222.0)
                        .context("08: work sidebar tab missing")?;
                    send_click(&mut harness, work_tab_pos, PointerButton::Primary, &ts);
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |h| {
                            semantic.ingest_from(h);
                            semantic.find_entry_center("notes.txt").is_some()
                                && semantic.find_entry_center("renamed.txt").is_some()
                        }),
                        "08: switching back to work tab at ({:.1},{:.1}) did not render work entries (state={:?}, notes={:?}, renamed={:?})",
                        work_tab_pos.x.0,
                        work_tab_pos.y.0,
                        browser.read_untracked(|state| (
                            state.active,
                            state
                                .tabs
                                .iter()
                                .map(|t| (
                                    t.id,
                                    t.navigation.location().label(),
                                    t.mode,
                                    t.rows.len()
                                ))
                                .collect::<Vec<_>>()
                        )),
                        semantic.find_entry_center("notes.txt"),
                        semantic.find_entry_center("renamed.txt")
                    );
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
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |h| {
                            semantic.ingest_from(h);
                            semantic.find_entry_center("notes.txt").is_some()
                                && semantic.find_entry_center("renamed.txt").is_none()
                        }),
                        "08: IME filter commit did not filter view"
                    );
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
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |_| {
                            browser.0.details.get_untracked() && browser.0.terminal.get_untracked()
                        }),
                        "09: details panel or terminal dock failed to open"
                    );
                    capture_checkpoint(
                        "09-details-terminal",
                        &mut harness,
                        &mut semantic,
                        &capture_state,
                        &artifacts,
                    )?;

                    // 10: Hierarchical tag address + mouse Back/Forward navigation.
                    let tag_root = TagAddress::new("work")
                        .map_err(|err| anyhow::anyhow!("invalid work tag: {err:?}"))?;
                    browser.open(Location::Tags(tag_root));
                    let tag_child = TagAddress::new("work/urgent")
                        .map_err(|err| anyhow::anyhow!("invalid work/urgent tag: {err:?}"))?;
                    browser.open(Location::Tags(tag_child));
                    browser.open(Location::from(&work));
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |_| {
                            browser.read_untracked(|state| {
                                state.tab().navigation.location().local_path() == Some(work.as_path())
                            })
                        }),
                        "10: failed to navigate back to work directory before history check"
                    );
                    send_click(
                        &mut harness,
                        Point::new(CssPx(640.0), CssPx(220.0)),
                        PointerButton::Back,
                        &ts,
                    );
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |_| {
                            browser.read_untracked(|state| {
                                matches!(state.tab().navigation.location(), Location::Tags(_))
                            })
                        }),
                        "10: mouse Back button did not return to tag location"
                    );
                    send_click(
                        &mut harness,
                        Point::new(CssPx(640.0), CssPx(220.0)),
                        PointerButton::Forward,
                        &ts,
                    );
                    ensure!(
                        wait_until(&mut harness, Duration::from_secs(2), |_| {
                            browser.read_untracked(|state| {
                                state.tab().navigation.location().local_path() == Some(work.as_path())
                            })
                        }),
                        "10: mouse Forward button did not return to work directory"
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
