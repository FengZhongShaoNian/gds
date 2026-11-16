//! Viewport transform, fit modes, cursor-anchored zoom, and coordinate conversions.

#[cfg(test)]
mod tests;

use gds::service::image::PixelRect;

pub const MIN_ZOOM: f32 = 0.05;
pub const MAX_ZOOM: f32 = 32.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point2 {
    pub x: f32,
    pub y: f32,
}

impl Point2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size2 {
    pub width: f32,
    pub height: f32,
}

impl Size2 {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect2 {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect2 {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Active viewport fitting policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FitMode {
    #[default]
    FitToWindow,
    ActualPixels,
    FillWidth,
    FillHeight,
    Custom,
}

/// Affine screen transform mapping `screen = image * scale + (pan_x, pan_y)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportTransform {
    pub scale: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub fit_mode: FitMode,
}

impl Default for ViewportTransform {
    fn default() -> Self {
        Self {
            scale: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            fit_mode: FitMode::FitToWindow,
        }
    }
}

impl ViewportTransform {
    pub fn compute_fit(mode: FitMode, viewport: Size2, image: Size2) -> Self {
        let vw = viewport.width.max(1.0);
        let vh = viewport.height.max(1.0);
        let iw = image.width.max(1.0);
        let ih = image.height.max(1.0);
        let raw_scale = match mode {
            FitMode::FitToWindow => (vw / iw).min(vh / ih),
            FitMode::ActualPixels | FitMode::Custom => 1.0,
            FitMode::FillWidth => vw / iw,
            FitMode::FillHeight => vh / ih,
        };
        let scale = raw_scale.clamp(MIN_ZOOM, MAX_ZOOM);
        let pan_x = (vw - iw * scale) * 0.5;
        let pan_y = (vh - ih * scale) * 0.5;
        Self {
            scale,
            pan_x,
            pan_y,
            fit_mode: mode,
        }
    }

    pub fn recenter_if_fitted(&mut self, viewport: Size2, image: Size2) {
        if self.fit_mode != FitMode::Custom {
            *self = Self::compute_fit(self.fit_mode, viewport, image);
        }
    }

    pub fn zoom_around_cursor(&mut self, factor: f32, cursor: Point2) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let anchor = self.viewport_to_image(cursor);
        let next_scale = (self.scale.max(MIN_ZOOM) * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.scale = next_scale;
        self.pan_x = cursor.x - anchor.x * next_scale;
        self.pan_y = cursor.y - anchor.y * next_scale;
        self.fit_mode = FitMode::Custom;
    }

    pub fn set_scale_centered(&mut self, scale: f32, viewport: Size2) {
        if !scale.is_finite() {
            return;
        }
        let center = Point2::new(viewport.width * 0.5, viewport.height * 0.5);
        let anchor = self.viewport_to_image(center);
        let next_scale = scale.clamp(MIN_ZOOM, MAX_ZOOM);
        self.scale = next_scale;
        self.pan_x = center.x - anchor.x * next_scale;
        self.pan_y = center.y - anchor.y * next_scale;
        self.fit_mode = FitMode::Custom;
    }

    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.pan_x += dx;
        self.pan_y += dy;
        self.fit_mode = FitMode::Custom;
    }

    pub fn viewport_to_image(&self, pt: Point2) -> Point2 {
        let scale = self.scale.max(MIN_ZOOM);
        Point2::new((pt.x - self.pan_x) / scale, (pt.y - self.pan_y) / scale)
    }

    pub fn image_to_viewport(&self, pt: Point2) -> Point2 {
        let scale = self.scale.max(MIN_ZOOM);
        Point2::new(pt.x * scale + self.pan_x, pt.y * scale + self.pan_y)
    }

    pub fn image_rect_to_viewport(&self, rect: Rect2) -> Rect2 {
        let scale = self.scale.max(MIN_ZOOM);
        let top_left = self.image_to_viewport(Point2::new(rect.x, rect.y));
        Rect2::new(
            top_left.x,
            top_left.y,
            rect.width * scale,
            rect.height * scale,
        )
    }

    pub fn normalize_drag_rect(start: Point2, end: Point2) -> Rect2 {
        let x0 = start.x.min(end.x);
        let y0 = start.y.min(end.y);
        let x1 = start.x.max(end.x);
        let y1 = start.y.max(end.y);
        Rect2::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }

    pub fn clamp_image_rect(rect: Rect2, image: Size2) -> Option<PixelRect> {
        let bounds_w = image.width.max(0.0).round() as u32;
        let bounds_h = image.height.max(0.0).round() as u32;
        let x0 = rect.x.max(0.0).floor() as u32;
        let y0 = rect.y.max(0.0).floor() as u32;
        let x1 = (rect.x + rect.width).max(0.0).ceil() as u32;
        let y1 = (rect.y + rect.height).max(0.0).ceil() as u32;
        let width = x1.saturating_sub(x0);
        let height = y1.saturating_sub(y0);
        PixelRect::new(x0, y0, width, height).clamp_to(bounds_w, bounds_h)
    }
}
