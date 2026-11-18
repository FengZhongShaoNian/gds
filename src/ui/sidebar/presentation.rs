use crate::appearance::tokens;

/// Host-sampled sidebar reveal geometry shared across sidebar sections.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SidebarPresentation {
    width: f32,
    progress: f32,
}

impl Default for SidebarPresentation {
    fn default() -> Self {
        Self {
            width: tokens::SIDEBAR_WIDTH,
            progress: 1.0,
        }
    }
}

impl SidebarPresentation {
    #[must_use]
    pub fn from_collapsed(collapsed: bool) -> Self {
        if collapsed {
            Self {
                width: tokens::COLLAPSED_SIDEBAR_WIDTH,
                progress: 0.0,
            }
        } else {
            Self::default()
        }
    }

    #[must_use]
    pub fn from_width(width: f32, expanded: f32, collapsed: f32) -> Self {
        let lo = collapsed.min(expanded);
        let hi = expanded.max(collapsed);
        let width = if width.is_finite() {
            width.clamp(lo, hi)
        } else {
            expanded
        };
        let span = expanded - collapsed;
        let progress = if span.abs() <= f32::EPSILON {
            1.0
        } else {
            ((width - collapsed) / span).clamp(0.0, 1.0)
        };
        Self { width, progress }
    }

    #[must_use]
    pub fn width(self) -> f32 {
        self.width
    }

    #[must_use]
    pub fn progress(self) -> f32 {
        self.progress
    }

    #[must_use]
    pub fn is_collapsed(self) -> bool {
        self.progress == 0.0
    }

    #[must_use]
    pub fn interpolate(self, expanded: f32, collapsed: f32) -> f32 {
        collapsed + (expanded - collapsed) * self.progress
    }

    #[must_use]
    pub fn label_opacity(self) -> f32 {
        self.progress
    }

    #[must_use]
    pub fn actions_visible(self) -> bool {
        self.progress == 1.0
    }
}
