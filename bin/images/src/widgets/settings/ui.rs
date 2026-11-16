//! Interactive settings widget owning lazy theme discovery and preference controls.

mod controls;
pub mod pages;

use std::path::PathBuf;

use gds::{
    appearance::{
        theme::{self, BuiltinTheme, ModePreference, Source, SourceKind},
        typography::{self, PreferencesEditor as FontEditor},
    },
    service::task_manager::{Owner, Scope},
    ui::{
        ContentPage, Frame, NavItem,
        settings::{PERCENTAGE_RANGE, form::Changed},
        sidebar::{SideBar, SideBarContent, SidebarPresentation, SidebarState},
    },
};
use gds_uim::el;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Global, IntoElement, KeyBinding, Render,
    SharedString, Subscription, Window,
    component::{WindowExt as _, scroll::ScrollableElement as _, slider::SliderValue},
    div,
    prelude::*,
    px,
};

use self::{
    controls::{
        ChoiceControl, ChoiceEvent, ChoiceState, Field, JPEG_QUALITY_RANGE, NumberControl,
        builtin_choices, custom_choices, mode_choices, source_choices,
    },
    pages::Page,
};
use crate::{
    entities::preferences::{
        DEFAULT_JPEG_QUALITY, MAX_INSPECTOR_WIDTH, MIN_INSPECTOR_WIDTH, Preferences,
    },
    shared::ui::actions::SETTINGS_CONTEXT,
};

gpui_kit::actions!(images_settings, [NextPage, PreviousPage]);

struct KeyBindingsRegistered;
impl Global for KeyBindingsRegistered {}

fn ensure_key_bindings(cx: &mut App) {
    if !cx.has_global::<KeyBindingsRegistered>() {
        cx.set_global(KeyBindingsRegistered);
        cx.bind_keys([
            KeyBinding::new("ctrl-tab", NextPage, Some(SETTINGS_CONTEXT)),
            KeyBinding::new("ctrl-shift-tab", PreviousPage, Some(SETTINGS_CONTEXT)),
        ]);
    }
}

fn normalized_source_kind(source: &Source, system_available: bool) -> SourceKind {
    match SourceKind::from(source) {
        SourceKind::System if !system_available => SourceKind::Builtin,
        other => other,
    }
}

pub enum Event {
    Apply(Preferences),
}

pub struct View {
    tasks: Owner,
    preferences: Preferences,
    sidebar_state: SidebarState,
    sidebar: SidebarPresentation,
    page: Page,
    page_focus: [FocusHandle; Page::ALL.len()],
    system_theme_available: bool,
    custom_themes: Vec<theme::provider::custom::Entry>,
    custom_themes_loading: bool,
    custom_scan_generation: u64,
    source: ChoiceControl<SourceKind>,
    builtin: ChoiceControl<BuiltinTheme>,
    custom: ChoiceControl<PathBuf>,
    mode: ChoiceControl<ModePreference>,
    font_editor: Entity<FontEditor>,
    content_opacity: NumberControl,
    sidebar_opacity: NumberControl,
    jpeg_quality: NumberControl,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Event> for View {}

impl View {
    pub fn new(
        tasks: Scope,
        preferences: Preferences,
        sidebar_state: SidebarState,
        sidebar: SidebarPresentation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        ensure_key_bindings(cx);
        let system_theme_available = Source::system_available(window.appearance());
        let source_kind =
            normalized_source_kind(&preferences.appearance.source, system_theme_available);
        let custom_path = match &preferences.appearance.source {
            Source::Custom(path) => path.clone(),
            _ => PathBuf::new(),
        };

        let source = ChoiceControl::new(
            source_choices(system_theme_available, cx),
            &source_kind,
            false,
            window,
            cx,
        );
        let builtin = ChoiceControl::new(
            builtin_choices(cx),
            &preferences.appearance.builtin,
            false,
            window,
            cx,
        );
        let custom = ChoiceControl::new(Vec::new(), &custom_path, true, window, cx);
        let mode = ChoiceControl::new(
            mode_choices(cx),
            &preferences.appearance.mode,
            false,
            window,
            cx,
        );
        let font_editor = cx.new(|cx| FontEditor::new(preferences.font.clone(), window, cx));
        let content_opacity = NumberControl::new(
            preferences.appearance.content_opacity * 100.0,
            PERCENTAGE_RANGE,
            cx,
        );
        let sidebar_opacity = NumberControl::new(
            preferences.appearance.sidebar_opacity * 100.0,
            PERCENTAGE_RANGE,
            cx,
        );
        let jpeg_quality = NumberControl::new(
            f32::from(preferences.default_jpeg_quality),
            JPEG_QUALITY_RANGE,
            cx,
        );

        let subscriptions = vec![
            cx.subscribe_in(&source.state, window, Self::on_source_changed),
            cx.subscribe_in(&builtin.state, window, Self::on_builtin_changed),
            cx.subscribe_in(&custom.state, window, Self::on_custom_changed),
            cx.subscribe_in(&mode.state, window, Self::on_mode_changed),
            cx.subscribe_in(&font_editor, window, Self::on_font_changed),
            Self::observe_slider(&content_opacity, Field::ContentOpacity, window, cx),
            Self::observe_slider(&sidebar_opacity, Field::SidebarOpacity, window, cx),
            Self::observe_slider(&jpeg_quality, Field::DefaultJpegQuality, window, cx),
        ];

        let mut view = Self {
            tasks: tasks.named_child("Settings"),
            preferences,
            sidebar_state,
            sidebar,
            page: Page::default(),
            page_focus: std::array::from_fn(|_| cx.focus_handle()),
            system_theme_available,
            custom_themes: Vec::new(),
            custom_themes_loading: false,
            custom_scan_generation: 0,
            source,
            builtin,
            custom,
            mode,
            font_editor,
            content_opacity,
            sidebar_opacity,
            jpeg_quality,
            _subscriptions: subscriptions,
        };
        view.refresh_themes(window, cx);
        view
    }

    fn observe_slider(
        control: &NumberControl,
        field: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.observe_in(&control.0.state, window, move |this, state, window, cx| {
            let SliderValue::Single(value) = state.read(cx).value() else {
                return;
            };
            let is_programmatic = match field {
                Field::ContentOpacity => this
                    .content_opacity
                    .0
                    .notification_is_programmatic(SliderValue::Single(value)),
                Field::SidebarOpacity => this
                    .sidebar_opacity
                    .0
                    .notification_is_programmatic(SliderValue::Single(value)),
                Field::DefaultJpegQuality => this
                    .jpeg_quality
                    .0
                    .notification_is_programmatic(SliderValue::Single(value)),
                _ => false,
            };
            if !is_programmatic {
                this.on_slider_changed(field, value, window, cx);
            }
        })
    }

    pub fn select_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        self.page = page;
        self.focus_active_page(window, cx);
        cx.notify();
    }

    pub fn focus_active_page(&self, window: &mut Window, cx: &mut App) {
        self.page_focus[self.page.index()].focus(window, cx);
    }

    pub fn set_sidebar_presentation(
        &mut self,
        state: SidebarState,
        presentation: SidebarPresentation,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_state != state || self.sidebar != presentation {
            self.sidebar_state = state;
            self.sidebar = presentation;
            cx.notify();
        }
    }

    pub fn set_preferences(
        &mut self,
        preferences: Preferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preferences == preferences {
            return;
        }
        self.preferences = preferences;
        self.sync_controls(window, cx);
    }

    fn sync_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source_kind = normalized_source_kind(
            &self.preferences.appearance.source,
            self.system_theme_available,
        );
        let custom_path = match &self.preferences.appearance.source {
            Source::Custom(path) => path.clone(),
            _ => PathBuf::new(),
        };
        self.source.sync(
            source_choices(self.system_theme_available, cx),
            &source_kind,
            window,
            cx,
        );
        self.builtin.sync(
            builtin_choices(cx),
            &self.preferences.appearance.builtin,
            window,
            cx,
        );
        self.custom.sync(
            custom_choices(&self.custom_themes, cx),
            &custom_path,
            window,
            cx,
        );
        self.mode.sync(
            mode_choices(cx),
            &self.preferences.appearance.mode,
            window,
            cx,
        );
        self.font_editor.update(cx, |editor, cx| {
            editor.set_value(self.preferences.font.clone(), window, cx);
        });
        self.content_opacity.sync(
            self.preferences.appearance.content_opacity * 100.0,
            window,
            cx,
        );
        self.sidebar_opacity.sync(
            self.preferences.appearance.sidebar_opacity * 100.0,
            window,
            cx,
        );
        self.jpeg_quality
            .sync(f32::from(self.preferences.default_jpeg_quality), window, cx);
        cx.notify();
    }

    pub fn refresh_themes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.custom_scan_generation = self.custom_scan_generation.wrapping_add(1);
        let generation = self.custom_scan_generation;
        self.system_theme_available = Source::system_available(window.appearance());
        let source_kind = normalized_source_kind(
            &self.preferences.appearance.source,
            self.system_theme_available,
        );
        self.source.sync(
            source_choices(self.system_theme_available, cx),
            &source_kind,
            window,
            cx,
        );
        self.custom_themes_loading = true;
        let work = self
            .tasks
            .spawn_blocking_named("Scan user themes", theme::provider::custom::catalog);
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.custom_scan_generation != generation {
                    return;
                }
                this.custom_themes_loading = false;
                if let Ok(Ok(entries)) = result {
                    this.custom_themes = entries;
                } else if let Ok(Err(error)) = result {
                    tracing::warn!(%error, "cannot scan custom themes directory");
                }
                let current_path = match &this.preferences.appearance.source {
                    Source::Custom(path) => path.clone(),
                    _ => PathBuf::new(),
                };
                this.custom.sync(
                    custom_choices(&this.custom_themes, cx),
                    &current_path,
                    window,
                    cx,
                );
                cx.notify();
            });
        })
        .detach();
    }

    fn emit_preferences(&mut self, cx: &mut Context<Self>) {
        self.preferences.default_jpeg_quality =
            self.preferences.default_jpeg_quality.clamp(10, 100);
        if let Some(width) = self.preferences.inspector_panel_width.as_mut() {
            *width = (*width).clamp(MIN_INSPECTOR_WIDTH, MAX_INSPECTOR_WIDTH);
        }
        cx.emit(Event::Apply(self.preferences.clone()));
        cx.notify();
    }

    fn on_source_changed(
        &mut self,
        _: &Entity<ChoiceState<SourceKind>>,
        event: &ChoiceEvent<SourceKind>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ChoiceEvent::Confirm(Some(kind)) = event else {
            return;
        };
        let next = match kind {
            SourceKind::System => Source::System,
            SourceKind::Builtin => Source::Builtin,
            SourceKind::Custom => {
                cx.notify();
                let first = self
                    .custom_themes
                    .first()
                    .map(|entry| entry.path.clone())
                    .unwrap_or_default();
                let current = match &self.preferences.appearance.source {
                    Source::Custom(path) if !path.as_os_str().is_empty() => path.clone(),
                    _ => first,
                };
                if current.as_os_str().is_empty() {
                    return;
                }
                Source::Custom(current)
            }
        };
        self.preferences.appearance.source = next;
        self.sync_controls(window, cx);
        self.emit_preferences(cx);
    }

    fn on_builtin_changed(
        &mut self,
        _: &Entity<ChoiceState<BuiltinTheme>>,
        event: &ChoiceEvent<BuiltinTheme>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let ChoiceEvent::Confirm(Some(builtin)) = event {
            self.preferences.appearance.builtin = *builtin;
            self.emit_preferences(cx);
        }
    }

    fn on_custom_changed(
        &mut self,
        _: &Entity<ChoiceState<PathBuf>>,
        event: &ChoiceEvent<PathBuf>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let ChoiceEvent::Confirm(Some(path)) = event {
            self.preferences.appearance.source = Source::Custom(path.clone());
            self.emit_preferences(cx);
        }
    }

    fn on_mode_changed(
        &mut self,
        _: &Entity<ChoiceState<ModePreference>>,
        event: &ChoiceEvent<ModePreference>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let ChoiceEvent::Confirm(Some(mode)) = event {
            self.preferences.appearance.mode = *mode;
            self.emit_preferences(cx);
        }
    }

    fn on_font_changed(
        &mut self,
        _: &Entity<FontEditor>,
        event: &Changed<typography::Preferences>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferences.font = event.value.clone();
        self.emit_preferences(cx);
    }

    fn on_slider_changed(
        &mut self,
        field: Field,
        value: f32,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (range, current) = match field {
            Field::ContentOpacity => (
                PERCENTAGE_RANGE,
                self.preferences.appearance.content_opacity * 100.0,
            ),
            Field::SidebarOpacity => (
                PERCENTAGE_RANGE,
                self.preferences.appearance.sidebar_opacity * 100.0,
            ),
            Field::DefaultJpegQuality => (
                JPEG_QUALITY_RANGE,
                f32::from(self.preferences.default_jpeg_quality),
            ),
            _ => return,
        };
        let quantized = range.quantize(value);
        if range.equivalent(current, quantized) {
            return;
        }
        match field {
            Field::ContentOpacity => {
                self.preferences.appearance.content_opacity = (quantized / 100.0).clamp(0.0, 1.0);
            }
            Field::SidebarOpacity => {
                self.preferences.appearance.sidebar_opacity = (quantized / 100.0).clamp(0.0, 1.0);
            }
            Field::DefaultJpegQuality => {
                self.preferences.default_jpeg_quality = (quantized.round() as u8).clamp(10, 100);
            }
            _ => return,
        }
        self.emit_preferences(cx);
    }

    fn set_toggle(&mut self, field: Field, checked: bool, _: &mut Window, cx: &mut Context<Self>) {
        match field {
            Field::RoundedCorners => self.preferences.appearance.rounded_corners = checked,
            Field::Animations => self.preferences.animations_enabled = Some(checked),
            Field::CheckerboardBackdrop => self.preferences.checkerboard_backdrop = checked,
            Field::StripPrivacyOnCopy => self.preferences.strip_privacy_on_copy = checked,
            _ => return,
        }
        self.emit_preferences(cx);
    }

    fn reset_field(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        field.reset(&mut self.preferences);
        if matches!(field, Field::DefaultJpegQuality) {
            self.preferences.default_jpeg_quality = DEFAULT_JPEG_QUALITY;
        }
        self.sync_controls(window, cx);
        self.emit_preferences(cx);
    }

    fn on_next_page(&mut self, _: &NextPage, window: &mut Window, cx: &mut Context<Self>) {
        self.select_page(self.page.step(1), window, cx);
    }

    fn on_previous_page(&mut self, _: &PreviousPage, window: &mut Window, cx: &mut Context<Self>) {
        self.select_page(self.page.step(-1), window, cx);
    }
}

impl Render for View {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let compact = self.sidebar_state == SidebarState::Collapsed;
        let presentation = self.sidebar;
        let active_page = self.page;
        let active_focus = self.page_focus[active_page.index()].clone();

        let nav_items = Page::ALL.into_iter().map(|page| {
            let label: SharedString = page.label(cx);
            let nav_id: gpui_kit::ElementId = ("settings-nav", page.index()).into();
            let icon = page.icon();
            let selected = page == active_page;
            let view_ref = entity.clone();
            el! {
                (nav_id, icon, label).NavItem::new {
                    compact: compact;
                    presentation: presentation;
                    selected: selected;
                    tooltip_text: $(page.label(cx));
                    tooltip_enabled: compact;
                    on_click: $(move |_, window, cx| {
                        view_ref.update(cx, |this, cx| this.select_page(page, window, cx));
                    });
                }
            }
        });

        let navigation = el! {
            .SideBar::default {
                state: $(self.sidebar_state);
                presentation: presentation;
            } [
                .SideBarContent::default { gap_1; px_2; py_2; } [
                    ..nav_items
                ]
            ]
        };

        let page_body = active_page.render(self, &entity, window, cx);
        let content = el! {
            .ContentPage::default [
                .div {
                    # "images-settings-scroll";
                    flex_1;
                    min_h_0;
                    min_w_0;
                    overflow_y_scrollbar;
                    track_focus: $(&active_focus);
                    tab_index: 0;
                    p_6;
                } [
                    .div {
                        mx_auto;
                        w_full;
                        max_w: $(px(760.0));
                        pb_8;
                    } [page_body]
                ]
            ]
        };

        let frame = Frame::default()
            .sidebar_width(presentation.width())
            .workspace(
                navigation.into_any_element(),
                content.into_any_element(),
                cx,
            );

        el! {
            .div {
                key_context: SETTINGS_CONTEXT;
                size_full;
                flex;
                flex_1;
                min_h_0;
                min_w_0;
                on_action: $(cx.listener(Self::on_next_page));
                on_action: $(cx.listener(Self::on_previous_page));
            } [frame]
        }
    }
}
