//! Terminal color semantics shared by painting and OSC color-query replies.

#[cfg(test)]
mod tests;

use alacritty_terminal::{
    term::{
        cell::Flags,
        color::{COUNT, Colors},
    },
    vte::ansi::{Color, NamedColor, Rgb},
};
use zgui::canvas::zgui_color::Color as ZguiColor;

use crate::{app::Gds, appearance::theme};

#[allow(dead_code)] // Seed skeleton for Wave 2 ui-terminal-tools.
pub(in crate::ui::terminal) struct Palette {
    pub background: ZguiColor,
    pub foreground: ZguiColor,
    pub selection: ZguiColor,
    pub selected_text: ZguiColor,
    pub cursor_text: ZguiColor,
    colors: [Rgb; COUNT],
}

#[allow(dead_code)]
impl Palette {
    pub fn current() -> Self {
        let gds = Gds::current();
        let theme = gds.appearance().theme();
        let roles = theme.colors().terminal;
        let alpha = theme.content_background().alpha();
        Self::from_roles(roles, alpha)
    }

    pub(crate) fn from_roles(roles: theme::TerminalColors, opacity: f32) -> Self {
        let background = roles.background.with_alpha(opacity);
        let mut colors = [Rgb { r: 0, g: 0, b: 0 }; COUNT];
        for i in 0..8 {
            colors[i] = to_rgb(roles.normal[i]);
            colors[i + 8] = to_rgb(roles.bright[i]);
            colors[NamedColor::DimBlack as usize + i] = dim(colors[i]);
        }
        for (i, color) in colors.iter_mut().enumerate().take(232).skip(16) {
            let n = i - 16;
            let level = |v: usize| if v == 0 { 0 } else { (55 + 40 * v) as u8 };
            *color = Rgb {
                r: level(n / 36),
                g: level(n / 6 % 6),
                b: level(n % 6),
            };
        }
        for (i, color) in colors.iter_mut().enumerate().take(256).skip(232) {
            let v = (8 + (i - 232) * 10) as u8;
            *color = Rgb { r: v, g: v, b: v };
        }
        colors[NamedColor::Foreground as usize] = to_rgb(roles.foreground);
        colors[NamedColor::Background as usize] = to_rgb(background);
        colors[NamedColor::Cursor as usize] = to_rgb(roles.cursor);
        colors[NamedColor::BrightForeground as usize] = to_rgb(roles.foreground);
        colors[NamedColor::DimForeground as usize] = dim(to_rgb(roles.foreground));
        Self {
            background,
            foreground: opaque(roles.foreground),
            selection: opaque(roles.selection),
            selected_text: opaque(roles.selection_text),
            cursor_text: opaque(roles.cursor_text),
            colors,
        }
    }

    pub fn query_colors(&self) -> [Rgb; COUNT] {
        self.colors
    }

    pub fn resolve(&self, color: Color, overrides: &Colors) -> ZguiColor {
        match color {
            Color::Spec(value) => from_rgb(value),
            Color::Named(index) => {
                from_rgb(overrides[index].unwrap_or(self.colors[index as usize]))
            }
            Color::Indexed(index) => {
                from_rgb(overrides[index as usize].unwrap_or(self.colors[index as usize]))
            }
        }
    }

    pub fn foreground(&self, color: Color, flags: Flags, overrides: &Colors) -> ZguiColor {
        let resolved = self.resolve(color, overrides);
        if flags.contains(Flags::DIM) {
            from_rgb(dim(to_rgb(resolved)))
        } else {
            resolved
        }
    }

    pub fn background(&self, overrides: &Colors) -> ZguiColor {
        self.resolve(Color::Named(NamedColor::Background), overrides)
            .with_alpha(self.background.alpha())
    }
}

fn opaque(color: ZguiColor) -> ZguiColor {
    color.with_alpha(1.0)
}

pub(crate) fn to_rgb(color: ZguiColor) -> Rgb {
    let [r, g, b] = color.components();
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    Rgb {
        r: channel(r),
        g: channel(g),
        b: channel(b),
    }
}

fn from_rgb(color: Rgb) -> ZguiColor {
    ZguiColor::srgb(
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        1.0,
    )
}

fn dim(color: Rgb) -> Rgb {
    let channel = |value: u8| (f32::from(value) * 0.66).round() as u8;
    Rgb {
        r: channel(color.r),
        g: channel(color.g),
        b: channel(color.b),
    }
}
