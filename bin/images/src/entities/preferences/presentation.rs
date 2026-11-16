//! Preference normalization and live presentation application before persistence.

use gds::{
    app::AppExt as _,
    appearance::{theme, typography},
};
use gpui_kit::App;

use super::{MAX_INSPECTOR_WIDTH, MIN_INSPECTOR_WIDTH, Preferences};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Theme(#[from] theme::Error),
    #[error(transparent)]
    Typography(#[from] typography::Error),
}

pub(super) struct Prepared {
    full_theme: Option<theme::Prepared>,
    opacity_update: Option<(f32, f32)>,
    corners_update: Option<bool>,
    font: Option<typography::Preferences>,
}

impl Prepared {
    pub(super) fn new(
        preferences: &Preferences,
        previous: &Preferences,
        cx: &App,
    ) -> Result<Self, Error> {
        let mut full_theme = None;
        let mut opacity_update = None;
        let mut corners_update = None;

        if preferences.appearance != previous.appearance {
            let palette_same = preferences.appearance.source == previous.appearance.source
                && preferences.appearance.builtin == previous.appearance.builtin
                && preferences.appearance.mode == previous.appearance.mode;
            if palette_same {
                if (preferences.appearance.content_opacity - previous.appearance.content_opacity)
                    .abs()
                    > f32::EPSILON
                    || (preferences.appearance.sidebar_opacity
                        - previous.appearance.sidebar_opacity)
                        .abs()
                        > f32::EPSILON
                {
                    opacity_update = Some((
                        preferences.appearance.content_opacity,
                        preferences.appearance.sidebar_opacity,
                    ));
                }
                if preferences.appearance.rounded_corners != previous.appearance.rounded_corners {
                    corners_update = Some(preferences.appearance.rounded_corners);
                }
            } else {
                full_theme = Some(preferences.appearance.prepare(cx.window_appearance())?);
            }
        }

        let font = (preferences.font != previous.font)
            .then(|| {
                preferences.font.validate()?;
                Ok::<_, typography::Error>(preferences.font.clone())
            })
            .transpose()?;

        Ok(Self {
            full_theme,
            opacity_update,
            corners_update,
            font,
        })
    }

    pub(super) fn install(self, preferences: &Preferences, cx: &mut App) {
        if let Some(theme) = self.full_theme {
            cx.gds_mut()
                .appearance()
                .theme()
                .install(theme)
                .expect("prepared theme has valid opacity");
        } else {
            if let Some((content, sidebar)) = self.opacity_update
                && let Err(error) = cx
                    .gds_mut()
                    .appearance()
                    .theme()
                    .update_opacity(content, sidebar)
            {
                tracing::warn!(%error, "cannot update surface opacity");
            }
            if let Some(rounded) = self.corners_update {
                cx.gds_mut()
                    .appearance()
                    .theme()
                    .update_window_corners(rounded);
            }
        }
        if let Some(font) = self.font {
            cx.gds_mut()
                .appearance()
                .typography()
                .apply(&font)
                .expect("prepared font has a valid size");
        }
        cx.gds_mut()
            .appearance()
            .motion()
            .set_enabled(preferences.animations_enabled.unwrap_or(true));
        cx.gds_mut()
            .appearance()
            .theme()
            .configure_hot_reload(preferences.appearance.clone());
    }
}

pub(super) fn normalize(preferences: &mut Preferences) {
    preferences.default_jpeg_quality = preferences.default_jpeg_quality.clamp(10, 100);
    if let Some(width) = &mut preferences.inspector_panel_width {
        *width = (*width).clamp(MIN_INSPECTOR_WIDTH, MAX_INSPECTOR_WIDTH);
    }
    let default_appearance = theme::Appearance::default();
    if !preferences.appearance.content_opacity.is_finite()
        || !(0.0..=1.0).contains(&preferences.appearance.content_opacity)
    {
        preferences.appearance.content_opacity = default_appearance.content_opacity;
    }
    if !preferences.appearance.sidebar_opacity.is_finite()
        || !(0.0..=1.0).contains(&preferences.appearance.sidebar_opacity)
    {
        preferences.appearance.sidebar_opacity = default_appearance.sidebar_opacity;
    }
    if !preferences.font.size.is_finite()
        || !(typography::MIN_SIZE..=typography::MAX_SIZE).contains(&preferences.font.size)
    {
        preferences.font.size = typography::DEFAULT_SIZE;
    }
}
