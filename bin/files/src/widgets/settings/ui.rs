//! Reactive settings pages editing the persisted application schema.

mod appearance;
mod pages;
mod tags;

use std::rc::Rc;

use gds::appearance::tokens::SIDEBAR_PADDING;
use gds::i18n;
use gds::ui::navigation::{NavItemLabelProps, NavItemProps};
use gds::ui::settings::form::{Changed, Editor, Model};
use gds::ui::settings::{
    NumberControlProps, PERCENTAGE_RANGE, SettingActionsProps, SettingControlProps,
    SettingDescriptionProps, SettingLabelProps, SettingResetProps, SettingRowProps,
};
use gds::ui::sidebar::SidebarPresentation;
use gds::ui::{IconButtonProps, IconName, IconProps};
use serde::{Serialize, de::DeserializeOwned};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, ReadSignal, RwSignal};
use zgui::view;
use zgui::view::install_stylesheet;
use zgui::vocab::{Key, NamedKey};
use zgui_ui::{SwitchProps, SwitchSize};
use zgui_ui_primitives::Binding;

use crate::entities::preferences::Preferences;
pub use pages::Page;

const SETTINGS_SHEET: &str = "gds-files-settings";
const SETTINGS_CSS: &str = r#"
.gds-settings-sidebar {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-height: 0;
    gap: 4px;
    padding: 8px;
}

.gds-settings-content {
    display: flex;
    flex-direction: column;
    flex: 1 1 0%;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    padding: 24px;
}

.gds-settings-heading {
    font-size: 20px;
    font-weight: 600;
    margin-bottom: 20px;
}

.gds-settings-col {
    display: flex;
    flex-direction: column;
    gap: 16px;
}

.gds-settings-slider-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-width: 0;
}

.gds-settings-slider-row__slider {
    flex: 1 1 0%;
    min-width: 0;
}

.gds-settings-slider-row__value {
    width: 56px;
    text-align: right;
    color: var(--gds-muted-foreground);
}

.gds-settings-footer {
    font-size: 12px;
    color: var(--gds-muted-foreground);
}
"#;

#[derive(Clone)]
pub struct View {
    preferences: ReadSignal<Preferences, LocalStorage>,
    on_apply: Rc<dyn Fn(Preferences)>,
    page: RwSignal<Page, LocalStorage>,
    themes: appearance::Themes,
    tags: tags::Probe,
    page_focus: NodeRef,
    performance_unlocked: RwSignal<bool, LocalStorage>,
}

impl View {
    pub fn new(
        preferences: ReadSignal<Preferences, LocalStorage>,
        on_apply: impl Fn(Preferences) + 'static,
    ) -> Self {
        Self {
            preferences,
            on_apply: Rc::new(on_apply),
            page: RwSignal::new_local(Page::General),
            themes: appearance::Themes::new(),
            tags: tags::Probe::new(preferences),
            page_focus: NodeRef::new(),
            performance_unlocked: RwSignal::new_local(false),
        }
    }

    fn apply(&self, change: impl FnOnce(&mut Preferences)) {
        let previous = self.preferences.get_untracked();
        let mut next = previous.clone();
        change(&mut next);
        self.themes.normalize_source(&mut next.appearance.source);
        let language_changed = next.language != previous.language;
        (self.on_apply)(next);
        if language_changed {
            self.page_focus.focus();
        }
    }

    fn subform<T>(&self, get: fn(&Preferences) -> T, set: fn(&mut Preferences, T)) -> Editor<T>
    where
        T: Model + Serialize + DeserializeOwned,
    {
        let preferences = self.preferences;
        let view = self.clone();
        let value = Binding::controlled(
            Signal::derive_local(move || preferences.with(get)),
            move |next| view.apply(|preferences| set(preferences, next)),
        );
        let apply_view = self.clone();
        Editor::new(value)
            .reset_focus(self.page_focus)
            .on_change(move |changed: Changed<T>| {
                apply_view.apply(|preferences| set(preferences, changed.value));
            })
    }

    fn reset<T: PartialEq + 'static>(
        &self,
        _label: &'static str,
        get: fn(&Preferences) -> T,
        set: fn(&mut Preferences, T),
    ) -> AnyView {
        let preferences = self.preferences;
        let view = self.clone();
        let focus = self.page_focus;
        AnyView::new(view! {
            SettingReset {
                {move || {
                    let modified = preferences.with(get) != get(&Preferences::default());
                    let view = view.clone();
                    modified.then(|| view! {
                        IconButton(
                            icon = IconName::RotateCcw,
                            label = Signal::derive_local(|| i18n::text("form-reset")),
                            on:click = move |_| {
                                view.apply(|preferences| set(preferences, get(&Preferences::default())));
                                focus.focus();
                            },
                        )
                    }.into_view())
                }}
            }
        })
    }

    fn toggle(
        &self,
        label: &'static str,
        help: &'static str,
        get: fn(&Preferences) -> bool,
        set: fn(&mut Preferences, bool),
    ) -> AnyView {
        let preferences = self.preferences;
        let view = self.clone();
        let value = Binding::controlled(
            Signal::derive_local(move || preferences.with(get)),
            move |checked| view.apply(|preferences| set(preferences, checked)),
        );
        let reset = self.reset(label, get, set);
        AnyView::new(view! {
            SettingRow {
                SettingLabel {
                    text {{move || i18n::text(label)}}
                    {(!help.is_empty()).then(|| view! {
                        SettingDescription {{move || i18n::text(help)}}
                    }.into_view())}
                }
                SettingActions {
                    {reset}
                    SettingControl {
                        Switch(checked = value, size = SwitchSize::Sm)
                    }
                }
            }
        })
    }

    fn opacity(
        &self,
        label: &'static str,
        get: fn(&Preferences) -> f32,
        set: fn(&mut Preferences, f32),
    ) -> AnyView {
        let preferences = self.preferences;
        let view = self.clone();
        let pct = Signal::derive_local(move || f64::from(preferences.with(get) * 100.0));
        let binding = Binding::controlled(pct, move |next| {
            view.apply(|p| set(p, (next as f32) / 100.0));
        });
        let reset = self.reset(label, get, set);
        AnyView::new(view! {
            SettingRow {
                SettingLabel {
                    text {{move || i18n::text(label)}}
                }
                SettingActions {
                    {reset}
                    SettingControl {
                        row(class = "gds-settings-slider-row") {
                            box(class = "gds-settings-slider-row__slider") {
                                NumberControl(
                                    value = binding,
                                    range = PERCENTAGE_RANGE,
                                    label = i18n::text(label),
                                )
                            }
                            text(class = "gds-settings-slider-row__value") {
                                {move || format!("{:.0}%", pct.get())}
                            }
                        }
                    }
                }
            }
        })
    }

    fn contents(&self) -> AnyView {
        match self.page.get() {
            Page::General => {
                let view = self.clone();
                let preferences = self.preferences;
                let editor = Editor::new(Binding::controlled(
                    Signal::derive_local(move || preferences.get()),
                    move |next| view.apply(|preferences| *preferences = next),
                ))
                .reset_focus(self.page_focus)
                .view();
                AnyView::new(view! {
                    column { {editor} }
                })
            }
            Page::Appearance => self.appearance(),
            Page::Files => AnyView::new(view! {
                column {
                    {self.toggle(
                        "sidebar-show-volumes",
                        "",
                        |p| p.files.sidebar.show_volumes,
                        |p, v| p.files.sidebar.show_volumes = v,
                    )}
                    {self.toggle(
                        "auto-focus-search",
                        "",
                        |p| p.files.auto_focus_search,
                        |p, v| p.files.auto_focus_search = v,
                    )}
                }
            }),
            Page::Terminal => {
                let preferences = self.preferences;
                let view = self.clone();
                AnyView::new(view! {
                    column(class = "gds-settings-col") {
                        {self.toggle(
                            "files-terminal-enabled",
                            "",
                            Preferences::terminal_enabled,
                            |p, v| p.terminal_enabled = Some(v),
                        )}
                        {move || preferences.with(Preferences::terminal_enabled).then(|| {
                            let terminal = view
                                .subform(|p| p.terminal.clone(), |p, v| p.terminal = v)
                                .view();
                            view! {
                                box { {terminal} }
                            }
                            .into_view()
                        })}
                    }
                })
            }
            Page::Extensions => self.extensions(),
            Page::Performance => self.performance(),
        }
    }

    fn adjacent(page: Page, backwards: bool) -> Page {
        let offset = if backwards { Page::ALL.len() - 1 } else { 1 };
        Page::ALL[(page.index() + offset) % Page::ALL.len()]
    }

    pub fn cycle_page(&self, backwards: bool) {
        self.page
            .set(Self::adjacent(self.page.get_untracked(), backwards));
        self.page_focus.focus();
    }

    /// Uses the host frame's same sidebar width; this never adds another surface.
    pub fn sidebar(&self, presentation: SidebarPresentation) -> AnyView {
        install_stylesheet(SETTINGS_SHEET, SETTINGS_CSS);
        let page = self.page;
        let focus = self.page_focus;
        let items = Page::ALL
            .into_iter()
            .map(|destination| {
                let selected = Signal::derive_local(move || page.get() == destination);
                view! {
                    NavItem(
                        selected = selected,
                        attr:data-testid = format!("settings-page-{}", destination.index()),
                        a11y:label = i18n::text(destination.label_id()),
                        on:click = move |_| {
                            page.set(destination);
                            focus.focus();
                        },
                    ) {
                        Icon(name = destination.icon())
                        if move || presentation.actions_visible() {
                            NavItemLabel {{move || i18n::text(destination.label_id())}}
                        } else {}
                    }
                }
                .into_view()
            })
            .collect::<Vec<_>>();
        AnyView::new(view! {
            scroll(
                class = "gds-settings-sidebar",
                style:padding = format!("{SIDEBAR_PADDING}px"),
            ) {
                {items}
            }
        })
    }

    pub fn content(&self) -> AnyView {
        install_stylesheet(SETTINGS_SHEET, SETTINGS_CSS);
        self.themes.refresh();
        let page = self.page;
        let focus = self.page_focus;
        let contents = self.clone();
        let navigation = self.clone();
        let focus_on_mount = zgui::reactive::RenderEffect::new(move |prev: Option<bool>| {
            let mounted = focus.get().is_some();
            if mounted && prev != Some(true) {
                focus.focus();
            }
            mounted
        });
        zgui::reactive::on_cleanup_local(move || drop(focus_on_mount));

        AnyView::new(view! {
            scroll(
                class = "gds-settings-content",
                node_ref = focus,
                tabindex = {Focus::Sequential},
                on:key_down = move |event| {
                    let modifiers = event.modifiers;
                    if modifiers.contains(zgui::vocab::Modifiers::PRIMARY)
                        && !modifiers.alt()
                        && event.key == Key::Named(NamedKey::Tab)
                    {
                        navigation.cycle_page(modifiers.shift());
                        event.prevent_default();
                        event.stop_propagation();
                    }
                },
            ) {
                text(class = "gds-settings-heading") {{move || i18n::text(page.get().label_id())}}
                {move || contents.contents()}
            }
        })
    }

    pub fn footer_text(&self) -> AnyView {
        install_stylesheet(SETTINGS_SHEET, SETTINGS_CSS);
        AnyView::new(view! {
            text(class = "gds-settings-footer") {{move || i18n::text("settings-auto-apply")}}
        })
    }
}

impl IntoView for View {
    type Output = AnyView;

    fn into_view(self) -> Self::Output {
        self.content()
    }
}
