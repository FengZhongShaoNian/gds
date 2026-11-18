//! Resolved GDS color roles shared across custom CSS properties and `zgui-ui-tokens`.
use std::fmt::Write as _;
use zgui::canvas::zgui_color::Color;
use zgui_ui_tokens::Theme;

use super::provider::color::color_css;
use crate::appearance::{motion, typography};

const HOVER_OPACITY: f32 = 0.08;
const PRESSED_OPACITY: f32 = 0.12;
const DRAG_OPACITY: f32 = 0.12;

/// Resolved GDS color roles shared by native components and custom surfaces.
/// Surface/foreground and action/foreground pairs must be used together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub terminal: super::TerminalColors,
    pub content: Color,
    pub sidebar: Color,
    pub foreground: Color,
    pub sidebar_foreground: Color,
    pub muted_foreground: Color,
    pub surface: Color,
    pub surface_foreground: Color,
    /// Raised panels, including the Agents control panel.
    pub panel: Color,
    pub panel_foreground: Color,
    /// Subtle separators; `outline` is the stronger control boundary.
    pub border: Color,
    pub outline: Color,
    pub shadow: Color,
    pub primary: Color,
    pub primary_foreground: Color,
    pub selection: Color,
    pub selection_foreground: Color,
    pub tooltip: Color,
    pub tooltip_foreground: Color,
    pub link: Color,
    pub error: Color,
    pub error_foreground: Color,
    /// Business status roles absent from Material exports use suite defaults.
    pub success: Color,
    pub success_foreground: Color,
    pub warning: Color,
    pub warning_foreground: Color,
}

impl Palette {
    /// Sources without terminal data share ANSI defaults and their own paired
    /// text/surface roles. Call after resolving the source's UI roles.
    pub(super) fn terminal_fallback(&mut self, mode: super::Mode) {
        self.terminal = super::TerminalColors {
            foreground: self.foreground,
            background: self.content,
            cursor: self.foreground,
            cursor_text: self.content,
            selection: self.selection,
            selection_text: self.selection_foreground,
            ..super::TerminalColors::fallback(mode)
        };
    }

    /// State layers keep neutral interaction distinct from persistent selection.
    pub fn hover(self) -> Color {
        self.foreground.with_alpha(HOVER_OPACITY)
    }

    pub fn pressed(self) -> Color {
        self.foreground.with_alpha(PRESSED_OPACITY)
    }

    pub fn sidebar_hover(self) -> Color {
        self.sidebar_foreground.with_alpha(HOVER_OPACITY)
    }

    pub fn drag_target(self) -> Color {
        self.primary.with_alpha(DRAG_OPACITY)
    }

    pub(crate) fn write_css_declarations(self, out: &mut String) {
        let colors = [
            ("--gds-content", self.content),
            ("--gds-sidebar", self.sidebar),
            ("--gds-foreground", self.foreground),
            ("--gds-sidebar-foreground", self.sidebar_foreground),
            ("--gds-muted-foreground", self.muted_foreground),
            ("--gds-surface", self.surface),
            ("--gds-surface-foreground", self.surface_foreground),
            ("--gds-panel", self.panel),
            ("--gds-panel-foreground", self.panel_foreground),
            ("--gds-border", self.border),
            ("--gds-outline", self.outline),
            ("--gds-shadow", self.shadow),
            ("--gds-scrim", self.shadow.with_alpha(0.5)),
            ("--gds-primary", self.primary),
            ("--gds-primary-foreground", self.primary_foreground),
            ("--gds-selection", self.selection),
            ("--gds-selection-foreground", self.selection_foreground),
            ("--gds-tooltip", self.tooltip),
            ("--gds-tooltip-foreground", self.tooltip_foreground),
            ("--gds-link", self.link),
            ("--gds-error", self.error),
            ("--gds-error-foreground", self.error_foreground),
            ("--gds-success", self.success),
            ("--gds-success-foreground", self.success_foreground),
            ("--gds-warning", self.warning),
            ("--gds-warning-foreground", self.warning_foreground),
            ("--gds-hover", self.hover()),
            ("--gds-pressed", self.pressed()),
            ("--gds-sidebar-hover", self.sidebar_hover()),
            ("--gds-drag-target", self.drag_target()),
        ];
        for (name, value) in colors {
            let _ = writeln!(out, "    {name}: {};", color_css(value));
        }
        self.terminal.write_css_declarations(out);
    }

    /// Lower this resolved GDS palette and appearance preferences into `zgui-ui-tokens::Theme`.
    pub fn tokens_theme(
        self,
        mode: super::Mode,
        typography: &typography::Resolved,
        motion: motion::Policy,
        rounded_corners: bool,
    ) -> Theme {
        let mut theme = match mode {
            super::Mode::Light => Theme::light(),
            super::Mode::Dark => Theme::dark(),
        };
        theme.color.background = color_css(self.content);
        theme.color.foreground = color_css(self.foreground);
        theme.color.card = color_css(self.surface);
        theme.color.card_foreground = color_css(self.surface_foreground);
        theme.color.popover = color_css(self.panel);
        theme.color.popover_foreground = color_css(self.panel_foreground);
        theme.color.tooltip = color_css(self.tooltip);
        theme.color.tooltip_foreground = color_css(self.tooltip_foreground);
        theme.color.tooltip_border = color_css(self.border);
        theme.color.primary = color_css(self.primary);
        theme.color.primary_foreground = color_css(self.primary_foreground);
        theme.color.secondary = color_css(self.surface);
        theme.color.secondary_foreground = color_css(self.surface_foreground);
        theme.color.muted = color_css(self.surface);
        theme.color.muted_foreground = color_css(self.muted_foreground);
        theme.color.accent = color_css(self.selection);
        theme.color.accent_foreground = color_css(self.selection_foreground);
        theme.color.destructive = color_css(self.error);
        theme.color.destructive_foreground = color_css(self.error_foreground);
        theme.color.danger = "var(--zui-color-destructive)".to_owned();
        theme.color.success = color_css(self.success);
        theme.color.success_foreground = color_css(self.success_foreground);
        theme.color.warning = color_css(self.warning);
        theme.color.warning_foreground = color_css(self.warning_foreground);
        theme.color.info = color_css(self.primary);
        theme.color.info_foreground = color_css(self.primary_foreground);
        theme.color.border = color_css(self.border);
        theme.color.input = color_css(self.outline);
        theme.color.ring = color_css(self.primary);
        theme.color.scrim = color_css(self.shadow.with_alpha(0.5));
        theme.color.sidebar = color_css(self.sidebar);
        theme.color.sidebar_foreground = color_css(self.sidebar_foreground);
        theme.color.sidebar_primary = color_css(self.primary);
        theme.color.sidebar_primary_foreground = color_css(self.primary_foreground);
        theme.color.sidebar_accent = color_css(self.selection);
        theme.color.sidebar_accent_foreground = color_css(self.selection_foreground);
        theme.color.sidebar_border = color_css(self.border);
        theme.color.sidebar_ring = color_css(self.primary);

        theme.control.ghost_hover = color_css(self.hover());
        theme.control.outline_hover = color_css(self.hover());

        let _ = rounded_corners;
        "4px".clone_into(&mut theme.radius.sm);
        "6px".clone_into(&mut theme.radius.md);
        "8px".clone_into(&mut theme.radius.lg);
        "10px".clone_into(&mut theme.radius.xl);
        "8px".clone_into(&mut theme.radius.base);

        theme.focus.disable_ring_shadows();
        "1px solid var(--zui-color-ring)".clone_into(&mut theme.focus.ring_outline);
        "-1px".clone_into(&mut theme.focus.ring_offset);

        let scale = typography.scale();
        theme.typography.family_sans = typography.sans_css_stack();
        theme.typography.family_mono = typography.mono_css_stack();
        theme.typography.size_xs = format!("{:.2}px", 12.0 * scale);
        theme.typography.size_sm = format!("{:.2}px", 14.0 * scale);
        theme.typography.size_md = format!("{:.2}px", 16.0 * scale);
        theme.typography.size_lg = format!("{:.2}px", 18.0 * scale);
        theme.typography.size_xl = format!("{:.2}px", 20.0 * scale);
        theme.typography.size_x2l = format!("{:.2}px", 24.0 * scale);
        theme.typography.size_x3l = format!("{:.2}px", 30.0 * scale);
        theme.typography.leading_xs = format!("{:.2}px", 16.0 * scale);
        theme.typography.leading_sm = format!("{:.2}px", 20.0 * scale);
        theme.typography.leading_md = format!("{:.2}px", 24.0 * scale);
        theme.typography.leading_lg = format!("{:.2}px", 28.0 * scale);
        theme.typography.leading_xl = format!("{:.2}px", 28.0 * scale);
        theme.typography.leading_x2l = format!("{:.2}px", 32.0 * scale);
        theme.typography.leading_x3l = format!("{:.2}px", 36.0 * scale);

        if motion.reduced() {
            "0ms".clone_into(&mut theme.motion.duration_fast);
            "0ms".clone_into(&mut theme.motion.duration_normal);
            "0ms".clone_into(&mut theme.motion.duration_slow);
            "0ms".clone_into(&mut theme.motion.duration_slower);
            "0ms".clone_into(&mut theme.motion.duration_slowest);
        } else {
            theme.motion.duration_normal = format!("{}ms", motion::WORKSPACE_DURATION_MS);
            theme.motion.ease_out = motion::WORKSPACE_EASING.to_owned();
        }
        theme
    }
}
