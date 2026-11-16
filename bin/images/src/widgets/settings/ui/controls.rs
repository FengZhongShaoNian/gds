//! Settings control primitives, field descriptors, and shared row builders.

use std::path::PathBuf;

use gds::{
    app::AppExt as _,
    appearance::theme::{self, BuiltinTheme, ModePreference, SourceKind},
    ui::{
        button::Button,
        field::select as select_field,
        settings::{
            CONTROL_WIDTH, NumberControl as NumericControl, NumberRange, PERCENTAGE_RANGE,
            SettingActions, SettingControl, SettingDescription, SettingLabel, SettingReset,
            SettingRow,
        },
    },
};
use gds_uim::el;
use gpui_kit::{
    App, ClickEvent, Context, ElementId, Entity, IntoElement, SharedString, Window,
    assets::IconName,
    base::StyledExt as _,
    component::{
        ActiveTheme as _, Disableable as _, IndexPath, Sizable as _,
        select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
        switch::Switch,
    },
    div,
    prelude::*,
    px,
};

use super::View;
use crate::{
    entities::preferences::{DEFAULT_JPEG_QUALITY, Preferences},
    shared::i18n::text,
};

pub(super) const JPEG_QUALITY_RANGE: NumberRange = NumberRange {
    min: 10.0,
    max: 100.0,
    step: 1.0,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Field {
    Source,
    Builtin,
    Custom,
    Mode,
    RoundedCorners,
    Animations,
    ContentOpacity,
    SidebarOpacity,
    CheckerboardBackdrop,
    StripPrivacyOnCopy,
    DefaultJpegQuality,
}

impl Field {
    pub const fn label_id(self) -> &'static str {
        match self {
            Self::Source => "theme-source",
            Self::Builtin => "theme-builtin-selection",
            Self::Custom => "theme-custom-selection",
            Self::Mode => "theme-mode",
            Self::RoundedCorners => "appearance-rounded-corners",
            Self::Animations => "appearance-animations",
            Self::ContentOpacity => "theme-opacity",
            Self::SidebarOpacity => "sidebar-opacity",
            Self::CheckerboardBackdrop => "pref-checkerboard",
            Self::StripPrivacyOnCopy => "pref-strip-on-copy",
            Self::DefaultJpegQuality => "pref-jpeg-quality",
        }
    }

    pub const fn description_id(self) -> Option<&'static str> {
        match self {
            Self::Animations => Some("appearance-animations-help"),
            Self::CheckerboardBackdrop => Some("pref-checkerboard-desc"),
            Self::StripPrivacyOnCopy => Some("pref-strip-on-copy-desc"),
            Self::DefaultJpegQuality => Some("pref-jpeg-quality-desc"),
            _ => None,
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Builtin => "builtin",
            Self::Custom => "custom",
            Self::Mode => "mode",
            Self::RoundedCorners => "rounded-corners",
            Self::Animations => "animations",
            Self::ContentOpacity => "content-opacity",
            Self::SidebarOpacity => "sidebar-opacity",
            Self::CheckerboardBackdrop => "checkerboard",
            Self::StripPrivacyOnCopy => "strip-on-copy",
            Self::DefaultJpegQuality => "jpeg-quality",
        }
    }

    pub fn row_id(self) -> ElementId {
        ("settings-row", self as u32).into()
    }

    pub fn control_id(self) -> ElementId {
        format!("settings-{}", self.slug()).into()
    }

    pub fn reset_id(self) -> ElementId {
        format!("settings-reset-{}", self.slug()).into()
    }

    pub fn is_default(self, prefs: &Preferences) -> bool {
        let defaults = Preferences::default();
        match self {
            Self::Source => {
                SourceKind::from(&prefs.appearance.source)
                    == SourceKind::from(&defaults.appearance.source)
            }
            Self::Builtin => prefs.appearance.builtin == defaults.appearance.builtin,
            Self::Custom => false,
            Self::Mode => prefs.appearance.mode == defaults.appearance.mode,
            Self::RoundedCorners => {
                prefs.appearance.rounded_corners == defaults.appearance.rounded_corners
            }
            Self::Animations => prefs.animations_enabled == defaults.animations_enabled,
            Self::ContentOpacity => PERCENTAGE_RANGE.equivalent(
                prefs.appearance.content_opacity * 100.0,
                defaults.appearance.content_opacity * 100.0,
            ),
            Self::SidebarOpacity => PERCENTAGE_RANGE.equivalent(
                prefs.appearance.sidebar_opacity * 100.0,
                defaults.appearance.sidebar_opacity * 100.0,
            ),
            Self::CheckerboardBackdrop => {
                prefs.checkerboard_backdrop == defaults.checkerboard_backdrop
            }
            Self::StripPrivacyOnCopy => {
                prefs.strip_privacy_on_copy == defaults.strip_privacy_on_copy
            }
            Self::DefaultJpegQuality => prefs.default_jpeg_quality == DEFAULT_JPEG_QUALITY,
        }
    }

    pub fn reset(self, prefs: &mut Preferences) {
        let defaults = Preferences::default();
        match self {
            Self::Source => prefs.appearance.source = defaults.appearance.source,
            Self::Builtin => prefs.appearance.builtin = defaults.appearance.builtin,
            Self::Custom => {}
            Self::Mode => prefs.appearance.mode = defaults.appearance.mode,
            Self::RoundedCorners => {
                prefs.appearance.rounded_corners = defaults.appearance.rounded_corners;
            }
            Self::Animations => prefs.animations_enabled = defaults.animations_enabled,
            Self::ContentOpacity => {
                prefs.appearance.content_opacity = defaults.appearance.content_opacity;
            }
            Self::SidebarOpacity => {
                prefs.appearance.sidebar_opacity = defaults.appearance.sidebar_opacity;
            }
            Self::CheckerboardBackdrop => {
                prefs.checkerboard_backdrop = defaults.checkerboard_backdrop;
            }
            Self::StripPrivacyOnCopy => {
                prefs.strip_privacy_on_copy = defaults.strip_privacy_on_copy;
            }
            Self::DefaultJpegQuality => prefs.default_jpeg_quality = DEFAULT_JPEG_QUALITY,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Choice<T> {
    pub value: T,
    pub label: SharedString,
    search: String,
}

impl<T> Choice<T> {
    pub fn new(value: T, label: SharedString, search: impl Into<String>) -> Self {
        Self {
            value,
            label,
            search: search.into(),
        }
    }
}

impl<T: Clone + PartialEq> SelectItem for Choice<T> {
    type Value = T;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_ascii_lowercase();
        q.is_empty()
            || self.label.to_ascii_lowercase().contains(&q)
            || self.search.to_ascii_lowercase().contains(&q)
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }
}

pub(super) fn source_choices(system_available: bool, cx: &App) -> Vec<Choice<SourceKind>> {
    let mut items = Vec::with_capacity(3);
    if system_available {
        items.push(Choice::new(
            SourceKind::System,
            text("theme-system", cx),
            "system follow desktop",
        ));
    }
    items.push(Choice::new(
        SourceKind::Builtin,
        text("theme-builtin", cx),
        "builtin preset",
    ));
    items.push(Choice::new(
        SourceKind::Custom,
        text("theme-custom", cx),
        "custom user file",
    ));
    items
}

pub(super) fn builtin_choices(cx: &App) -> Vec<Choice<BuiltinTheme>> {
    BuiltinTheme::ALL
        .into_iter()
        .map(|theme| Choice::new(theme, text(theme.label_id(), cx), theme.label_id()))
        .collect()
}

pub(super) fn custom_choices(
    entries: &[theme::provider::custom::Entry],
    cx: &App,
) -> Vec<Choice<PathBuf>> {
    let locale = cx.gds().i18n().language().locale();
    entries
        .iter()
        .map(|entry| {
            let name = entry.metadata.display_name(locale);
            let search = format!("{name} {}", entry.path.display());
            Choice::new(entry.path.clone(), name.to_owned().into(), search)
        })
        .collect()
}

pub(super) fn mode_choices(cx: &App) -> Vec<Choice<ModePreference>> {
    vec![
        Choice::new(
            ModePreference::System,
            text("theme-mode-system", cx),
            "system",
        ),
        Choice::new(ModePreference::Light, text("theme-light", cx), "light"),
        Choice::new(ModePreference::Dark, text("theme-dark", cx), "dark"),
    ]
}

pub(super) type ChoiceEvent<T> = SelectEvent<SearchableVec<Choice<T>>>;
pub(super) type ChoiceState<T> = SelectState<SearchableVec<Choice<T>>>;

pub(super) struct ChoiceControl<T: Clone + PartialEq + 'static> {
    pub state: Entity<ChoiceState<T>>,
    choices: Vec<Choice<T>>,
}

impl<T: Clone + PartialEq + 'static> ChoiceControl<T> {
    pub fn new(
        choices: Vec<Choice<T>>,
        current: &T,
        searchable: bool,
        window: &mut Window,
        cx: &mut Context<View>,
    ) -> Self {
        let selected = choices
            .iter()
            .position(|choice| &choice.value == current)
            .map(|row| IndexPath::default().row(row));
        let items = SearchableVec::new(choices.clone());
        let state =
            cx.new(|cx| SelectState::new(items, selected, window, cx).searchable(searchable));
        Self { state, choices }
    }

    pub fn sync(
        &mut self,
        choices: Vec<Choice<T>>,
        current: &T,
        window: &mut Window,
        cx: &mut Context<View>,
    ) {
        if self.choices != choices {
            self.state.update(cx, |state, cx| {
                state.set_items(SearchableVec::new(choices.clone()), window, cx);
            });
            self.choices = choices;
        }
        let expected = self
            .choices
            .iter()
            .find(|choice| &choice.value == current)
            .map(|choice| choice.value.clone());
        if self.state.read(cx).selected_value().cloned() == expected {
            return;
        }
        self.state.update(cx, |state, cx| match &expected {
            Some(value) => state.set_selected_value(value, window, cx),
            None => state.set_selected_index(None, window, cx),
        });
    }

    pub fn element(
        &self,
        field: Field,
        disabled: bool,
        cx: &App,
    ) -> Select<SearchableVec<Choice<T>>> {
        let placeholder = match field {
            Field::Builtin => text("theme-builtin-selection", cx),
            Field::Custom => text("theme-custom-selection", cx),
            _ => text(field.label_id(), cx),
        };
        el! {
            ($(&self.state), cx).select_field {
                # $("settings-select", field as usize);
                small;
                w_full;
                menu_width: CONTROL_WIDTH.px;
                disabled: disabled;
                accessibility_label: $(text(field.label_id(), cx));
                placeholder: placeholder;
                search_placeholder: $(text("theme-search", cx));
            }
        }
    }
}

pub(super) struct NumberControl(pub(super) NumericControl);

impl NumberControl {
    pub fn new(value: f32, range: NumberRange, cx: &mut Context<View>) -> Self {
        Self(NumericControl::new(value, range, cx))
    }

    pub fn sync(&self, value: f32, window: &mut Window, cx: &mut Context<View>) {
        self.0.sync(value, window, cx);
    }

    pub fn row(
        &self,
        field: Field,
        value_label: SharedString,
        prefs: &Preferences,
        view: &Entity<View>,
        cx: &App,
    ) -> impl IntoElement {
        let label = text(field.label_id(), cx);
        let control = self
            .0
            .element(field.control_id(), label, value_label, false);
        setting_row(
            field,
            control,
            !field.is_default(prefs),
            reset_handler(view, field),
            cx,
        )
    }
}

pub(super) fn reset_handler(
    view: &Entity<View>,
    field: Field,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let view = view.clone();
    move |_, window, cx| {
        view.update(cx, |this, cx| this.reset_field(field, window, cx));
    }
}

pub(super) fn toggle_row(
    view: &Entity<View>,
    field: Field,
    checked: bool,
    disabled: bool,
    prefs: &Preferences,
    cx: &App,
) -> impl IntoElement {
    let switch_id = field.control_id();
    let label = text(field.label_id(), cx);
    let view_ref = view.clone();
    let control = el! {
        switch_id.Switch::new {
            small;
            checked: checked;
            disabled: disabled;
            accessibility_label: label;
            on_change: $(move |next: &bool, window, cx| {
                view_ref.update(cx, |this, cx| this.set_toggle(field, *next, window, cx));
            });
        }
    };
    setting_row(
        field,
        control,
        !field.is_default(prefs),
        reset_handler(view, field),
        cx,
    )
}

pub(super) fn select_row<T: Clone + PartialEq + 'static>(
    view: &Entity<View>,
    field: Field,
    choice: &ChoiceControl<T>,
    disabled: bool,
    prefs: &Preferences,
    cx: &App,
) -> impl IntoElement {
    setting_row(
        field,
        choice.element(field, disabled, cx),
        !field.is_default(prefs),
        reset_handler(view, field),
        cx,
    )
}

pub(super) fn setting_row(
    field: Field,
    control: impl IntoElement,
    show_reset: bool,
    on_reset: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let label = text(field.label_id(), cx);
    let description = field.description_id().map(|id| text(id, cx));
    let reset_label = text("settings-reset-default", cx);
    let reset_id = field.reset_id();
    el! {
        .SettingRow::default {
            # $(field.row_id());
        } [
            .SettingLabel::default {
                font_medium;
                text_color: $(cx.theme().foreground);
            } [
                label,
                @if (let Some(desc) = description)
                    .SettingDescription::default [desc],
                @end,
            ],
            .SettingActions::default [
                .SettingReset::default [
                    @if (show_reset)
                        reset_id.Button::icon_button {
                            control_icon: IconName::RotateCcw;
                            accessibility_label: $(reset_label.clone());
                            tooltip: reset_label;
                            on_click: on_reset;
                        },
                    @end,
                ],
                .SettingControl::default [control],
            ],
        ]
    }
}

pub(super) fn section_heading(title: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    let title: SharedString = title.into();
    el! {
        .div {
            text_base;
            font_semibold;
            text_color: $(cx.theme().foreground);
            pb_2;
            border_b_1;
            border_color: $(cx.theme().border);
        } [title]
    }
}

pub(super) fn format_percent(fraction: f32) -> SharedString {
    format!("{:.0}%", (fraction * 100.0).clamp(0.0, 100.0)).into()
}

pub(super) fn format_quality(quality: u8) -> SharedString {
    format!("{quality}%").into()
}
