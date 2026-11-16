//! Annotation primitives mapped cleanly onto `gds::service::image::ops::RasterOp`.

use gds::service::image::{PixelRect, ops::RasterOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnnotationId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaColor {
    pub const RED: Self = Self::rgb(239, 68, 68);
    pub const AMBER: Self = Self::rgb(245, 158, 11);
    pub const GREEN: Self = Self::rgb(34, 197, 94);
    pub const BLUE: Self = Self::rgb(59, 130, 246);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const BLACK: Self = Self::rgb(17, 24, 39);

    pub const PALETTE: [Self; 6] = [
        Self::RED,
        Self::AMBER,
        Self::GREEN,
        Self::BLUE,
        Self::WHITE,
        Self::BLACK,
    ];

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AnnotationKind {
    Rect {
        rect: PixelRect,
        stroke: RgbaColor,
        stroke_width: f32,
        fill: Option<RgbaColor>,
    },
    Ellipse {
        rect: PixelRect,
        stroke: RgbaColor,
        stroke_width: f32,
        fill: Option<RgbaColor>,
    },
    Arrow {
        start: [f32; 2],
        end: [f32; 2],
        color: RgbaColor,
        stroke_width: f32,
    },
    Line {
        start: [f32; 2],
        end: [f32; 2],
        color: RgbaColor,
        stroke_width: f32,
    },
    Text {
        position: [f32; 2],
        content: String,
        color: RgbaColor,
        font_size: f32,
    },
    Mosaic {
        region: PixelRect,
        block_size: u32,
    },
    GaussianBlur {
        region: PixelRect,
        radius: u32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnnotationItem {
    pub id: AnnotationId,
    pub kind: AnnotationKind,
}

impl AnnotationItem {
    pub fn to_raster_op(&self) -> RasterOp {
        match &self.kind {
            AnnotationKind::Rect {
                rect,
                stroke,
                stroke_width,
                fill,
            } => RasterOp::Rect {
                x: rect.x as f32,
                y: rect.y as f32,
                width: rect.width as f32,
                height: rect.height as f32,
                color: stroke.to_array(),
                stroke_width: *stroke_width,
                fill: fill.map(RgbaColor::to_array),
            },
            AnnotationKind::Ellipse {
                rect,
                stroke,
                stroke_width,
                fill,
            } => RasterOp::Ellipse {
                x: rect.x as f32,
                y: rect.y as f32,
                width: rect.width as f32,
                height: rect.height as f32,
                color: stroke.to_array(),
                stroke_width: *stroke_width,
                fill: fill.map(RgbaColor::to_array),
            },
            AnnotationKind::Arrow {
                start,
                end,
                color,
                stroke_width,
            } => RasterOp::Arrow {
                start: *start,
                end: *end,
                color: color.to_array(),
                stroke_width: *stroke_width,
            },
            AnnotationKind::Line {
                start,
                end,
                color,
                stroke_width,
            } => RasterOp::Line {
                start: *start,
                end: *end,
                color: color.to_array(),
                stroke_width: *stroke_width,
            },
            AnnotationKind::Text {
                position,
                content,
                color,
                font_size,
            } => RasterOp::Text {
                position: *position,
                content: content.clone(),
                font_size: *font_size,
                color: color.to_array(),
            },
            AnnotationKind::Mosaic { region, block_size } => RasterOp::Mosaic {
                region: *region,
                block_size: *block_size,
            },
            AnnotationKind::GaussianBlur { region, radius } => RasterOp::GaussianBlur {
                region: *region,
                radius: *radius,
            },
        }
    }
}
