//! Resolve a real monospace family before measuring the terminal grid.
//!
//! GPUI treats unknown family names as ordinary font fallbacks; the CSS name
//! `monospace` is not a portable font family in its public text API.
use std::sync::Arc;

use gpui_kit::{Font, FontFeatures, Pixels, Size, TextSystem, font, px, size};

#[derive(Clone)]
pub(super) struct Fonts {
    requested: String,
    scale: f32,
    pub font: Font,
    pub size: Pixels,
    pub cell: Size<Pixels>,
    pub baseline: Pixels,
}

impl Fonts {
    pub fn matches(&self, family: &str, size: f32, scale: f32) -> bool {
        self.requested == family && self.size == px(size) && self.scale == scale
    }

    pub fn resolve(requested: &str, font_size: f32, scale: f32, text: &TextSystem) -> Self {
        let family = crate::appearance::typography::resolve_monospace_family(requested, text);
        let mut font = font(family);
        // Terminal columns are owned by the emulator, not font ligatures.
        font.features = FontFeatures(Arc::new(vec![
            ("calt".into(), 0),
            ("liga".into(), 0),
            ("clig".into(), 0),
        ]));
        let id = text.resolve_font(&font);
        let font_size = px(font_size);
        let advance = text
            .advance(id, font_size, '0')
            .map(|s| s.width)
            .unwrap_or_else(|_| text.layout_width(id, font_size, '0'));
        let ascent = text.ascent(id, font_size);
        let descent = text.descent(id, font_size).abs();
        let scale = scale.max(1.);
        // Like Alacritty, use device-pixel cell dimensions and one common baseline.
        let width = px((f32::from(advance) * scale).floor().max(1.) / scale);
        let height = px(((f32::from(ascent + descent)) * scale).ceil().max(1.) / scale);
        let baseline = px((f32::from(ascent) * scale).round() / scale);
        Self {
            requested: requested.into(),
            scale,
            font,
            size: font_size,
            cell: size(width, height),
            baseline,
        }
    }
}
