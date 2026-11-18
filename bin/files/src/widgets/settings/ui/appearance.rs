use std::path::PathBuf;

use gds::ui::settings::{
    SettingActionsProps, SettingControlProps, SettingDescriptionProps, SettingLabelProps,
    SettingResetProps, SettingRowProps,
};
use gds::ui::{IconButtonProps, IconName};
use gds::{
    app::{Gds, appearance::Theme},
    appearance::theme::{self, Source, SourceKind},
    i18n,
};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::view;
use zgui::view::install_stylesheet;
use zgui_ui::{
    ComboboxContentProps, ComboboxEmptyProps, ComboboxInputProps, ComboboxItemProps, ComboboxProps,
};
use zgui_ui_primitives::Binding;

use super::{Preferences, View};

type Entries = Vec<theme::provider::custom::Entry>;

const APPEARANCE_SHEET: &str = "gds-files-settings-appearance";
const APPEARANCE_CSS: &str = r#"
.gds-settings-appearance {
    display: flex;
    flex-direction: column;
}

.gds-settings-appearance__sample {
    padding: 16px 0;
    border-bottom: 1px solid var(--gds-border);
    color: var(--gds-muted-foreground);
    font-size: calc(var(--gds-font-size, 14px) * 1.28);
}

.gds-settings-appearance__help {
    font-size: 13px;
}
"#;

#[derive(Clone)]
pub(super) struct Themes {
    system_available: RwSignal<Option<bool>, LocalStorage>,
    catalogue: RwSignal<Result<Entries, String>, LocalStorage>,
    custom_pending: RwSignal<bool, LocalStorage>,
}

impl Themes {
    pub(super) fn new() -> Self {
        Self {
            system_available: RwSignal::new_local(None),
            catalogue: RwSignal::new_local(Ok(Vec::new())),
            custom_pending: RwSignal::new_local(false),
        }
    }

    pub(super) fn refresh(&self) {
        let available = self.system_available;
        let catalogue = self.catalogue;
        let mode = Theme::platform_mode();
        let scope = Gds::current().tasks().owner_scope("Available themes");
        let work = scope.spawn_blocking_named("Discover themes", move || {
            (
                Source::system_available(mode),
                theme::provider::custom::catalog(),
            )
        });
        Gds::current()
            .tasks()
            .spawn_ui("Deliver theme catalog", work, move |result| {
                match result.and_then(std::convert::identity) {
                    Ok((system, entries)) => {
                        if available.with_untracked(|cur| *cur != Some(system)) {
                            available.set(Some(system));
                        }
                        let next = entries.map_err(|error| error.to_string());
                        if catalogue.with_untracked(|cur| *cur != next) {
                            catalogue.set(next);
                        }
                    }
                    Err(error) => {
                        let msg = error.to_string();
                        catalogue.set(Err(msg));
                    }
                }
            });
    }

    pub(super) fn normalize_source(&self, source: &mut Source) {
        if self.system_available.get_untracked() == Some(false) && *source == Source::System {
            *source = Source::Builtin;
        }
    }
}

#[derive(Clone)]
struct ComboOption {
    value: String,
    label: String,
    search_text: String,
    disabled: bool,
}

fn combo_item(opt: ComboOption) -> AnyView {
    let label = Signal::stored_local(opt.label);
    AnyView::new(view! {
        ComboboxItem(
            value = opt.value,
            text = opt.search_text,
            disabled = opt.disabled,
        ) {{move || label.get()}}
    })
}

impl View {
    fn theme_choice(
        label: &'static str,
        reset: AnyView,
        value: Signal<String, LocalStorage>,
        options: Vec<ComboOption>,
        on_pick: impl Fn(String) + 'static,
    ) -> AnyView {
        let list = Signal::stored_local(options);

        AnyView::new(view! {
            SettingRow {
                SettingLabel {
                    text {{move || i18n::text(label)}}
                }
                SettingActions {
                    {reset}
                    SettingControl {
                        Combobox(value = Binding::controlled(value, on_pick)) {
                            ComboboxInput(
                                placeholder = i18n::text("theme-search"),
                                label = i18n::text(label),
                            ) {}
                            ComboboxContent {
                                for opt in move || list.get(), key = |o| o.value.clone() {
                                    {combo_item(opt)}
                                }
                                ComboboxEmpty {{move || i18n::text("no-matches")}}
                            }
                        }
                    }
                }
            }
        })
    }

    fn theme_source(&self) -> AnyView {
        let preferences = self.preferences;
        let themes = self.themes.clone();
        let available = themes.system_available.get();
        let pending = themes.custom_pending;
        let options = [
            (SourceKind::System, "theme-system", "system 系统"),
            (SourceKind::Builtin, "theme-builtin", "builtin 内置"),
            (SourceKind::Custom, "theme-custom", "custom 自定义"),
        ]
        .into_iter()
        .map(|(kind, label, keywords)| {
            let text = i18n::text(label);
            ComboOption {
                value: source_kind_key(kind).to_owned(),
                search_text: format!("{text} {keywords}"),
                label: text,
                disabled: kind == SourceKind::System && available != Some(true),
            }
        })
        .collect();

        let selected_kind = Signal::derive_local(move || {
            let kind = if pending.get() {
                SourceKind::Custom
            } else {
                preferences.with(|p| match &p.appearance.source {
                    Source::System if available != Some(true) => SourceKind::Builtin,
                    source => SourceKind::from(source),
                })
            };
            source_kind_key(kind).to_owned()
        });

        let view = self.clone();
        let apply_view = self.clone();
        let reset_view = self.clone();
        let focus = self.page_focus;
        let reset = AnyView::new(view! {
            SettingReset {
                {move || {
                    let modified = pending.get()
                        || preferences.with(|p| p.appearance.source != Source::default());
                    let reset_view = reset_view.clone();
                    modified.then(|| view! {
                        IconButton(
                            icon = IconName::RotateCcw,
                            label = Signal::derive_local(|| i18n::text("form-reset")),
                            attr:data-testid = "theme-source-reset",
                            on:click = move |_| {
                                pending.set(false);
                                reset_view.apply(|p| p.appearance.source = Source::default());
                                focus.focus();
                            },
                        )
                    }.into_view())
                }}
            }
        });

        let header =
            Self::theme_choice("theme-source", reset, selected_kind, options, move |key| {
                let kind = parse_source_kind(&key);
                pending.set(kind == SourceKind::Custom);
                match kind {
                    SourceKind::System => {
                        apply_view.apply(|p| p.appearance.source = Source::System)
                    }
                    SourceKind::Builtin => {
                        apply_view.apply(|p| p.appearance.source = Source::Builtin)
                    }
                    SourceKind::Custom => {}
                }
            });

        AnyView::new(view! {
            column(class = "gds-settings-appearance") {
                {header}
                {move || {
                    if pending.get() {
                        view.custom_choice()
                    } else {
                        match preferences.with(|p| match &p.appearance.source {
                            Source::System if available != Some(true) => SourceKind::Builtin,
                            source => SourceKind::from(source),
                        }) {
                            SourceKind::System => AnyView::new(view! { box() }),
                            SourceKind::Builtin => {
                                let builtin_options = theme::BuiltinTheme::ALL
                                    .into_iter()
                                    .map(|entry| {
                                        let label = i18n::text(entry.label_id());
                                        ComboOption {
                                            value: serde_json::to_string(&entry).unwrap_or_default(),
                                            search_text: format!("{label} {}", entry.label_id()),
                                            label,
                                            disabled: false,
                                        }
                                    })
                                    .collect();
                                view.palette_choice(
                                    "theme-builtin-selection",
                                    |p| p.appearance.builtin,
                                    |p, v| p.appearance.builtin = v,
                                    builtin_options,
                                )
                            }
                            SourceKind::Custom => view.custom_choice(),
                        }
                    }
                }}
            }
        })
    }

    fn custom_choice(&self) -> AnyView {
        let preferences = self.preferences;
        let view = self.clone();
        let selected = Signal::derive_local(move || {
            preferences.with(|p| match &p.appearance.source {
                Source::Custom(path) => path.to_string_lossy().into_owned(),
                _ => String::new(),
            })
        });
        let locale = Gds::current().i18n().language();
        let mut options = self.themes.catalogue.with(|catalogue| {
            catalogue.as_ref().ok().map_or_else(Vec::new, |entries| {
                entries
                    .iter()
                    .map(|entry| {
                        let name = entry.metadata.display_name(locale.locale()).to_owned();
                        ComboOption {
                            value: entry.path.to_string_lossy().into_owned(),
                            search_text: name.clone(),
                            label: name,
                            disabled: false,
                        }
                    })
                    .collect()
            })
        });
        let current_path = selected.get_untracked();
        if !current_path.is_empty() && !options.iter().any(|item| item.value == current_path) {
            options.push(ComboOption {
                value: current_path.clone(),
                search_text: current_path.clone(),
                label: current_path,
                disabled: false,
            });
        }
        let error = self
            .themes
            .catalogue
            .with(|entries| entries.as_ref().err().cloned());
        let chooser = Self::theme_choice(
            "theme-custom-selection",
            AnyView::new(view! { SettingReset {} }),
            selected,
            options,
            move |path| {
                view.themes.custom_pending.set(false);
                view.apply(|p| p.appearance.source = Source::Custom(PathBuf::from(path)));
            },
        );
        AnyView::new(view! {
            column {
                {chooser}
                {error.map(|message| view! {
                    SettingDescription {{message}}
                }.into_view())}
            }
        })
    }

    fn palette_choice<T>(
        &self,
        label: &'static str,
        get: fn(&Preferences) -> T,
        set: fn(&mut Preferences, T),
        options: Vec<ComboOption>,
    ) -> AnyView
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + 'static,
    {
        let preferences = self.preferences;
        let view = self.clone();
        let value = Signal::derive_local(move || {
            serde_json::to_string(&preferences.with(get)).unwrap_or_default()
        });
        Self::theme_choice(
            label,
            self.reset(label, get, set),
            value,
            options,
            move |raw| {
                if let Ok(parsed) = serde_json::from_str::<T>(&raw) {
                    view.apply(|preferences| set(preferences, parsed));
                }
            },
        )
    }

    pub(super) fn appearance(&self) -> AnyView {
        install_stylesheet(APPEARANCE_SHEET, APPEARANCE_CSS);
        let source = self.clone();
        let mode = self.clone();
        let font = self.subform(|p| p.font.clone(), |p, v| p.font = v).view();
        AnyView::new(view! {
            column(class = "gds-settings-appearance") {
                {move || source.theme_source()}
                {move || {
                    let options = [
                        (theme::ModePreference::System, "theme-mode-system", "system automatic 系统"),
                        (theme::ModePreference::Light, "theme-light", "light 浅色"),
                        (theme::ModePreference::Dark, "theme-dark", "dark 深色"),
                    ]
                    .into_iter()
                    .map(|(entry, label, keywords)| {
                        let text = i18n::text(label);
                        ComboOption {
                            value: serde_json::to_string(&entry).unwrap_or_default(),
                            search_text: format!("{text} {keywords}"),
                            label: text,
                            disabled: false,
                        }
                    })
                    .collect();
                    mode.palette_choice(
                        "theme-mode",
                        |p| p.appearance.mode,
                        |p, v| p.appearance.mode = v,
                        options,
                    )
                }}
                {self.toggle(
                    "appearance-rounded-corners",
                    "",
                    |p| p.appearance.rounded_corners,
                    |p, v| p.appearance.rounded_corners = v,
                )}
                {self.toggle(
                    "appearance-animations",
                    "appearance-animations-help",
                    |p| p.animations_enabled.unwrap_or(true),
                    |p, v| p.animations_enabled = Some(v),
                )}
                {self.opacity(
                    "theme-opacity",
                    |p| p.appearance.content_opacity,
                    |p, v| p.appearance.content_opacity = v,
                )}
                {self.opacity(
                    "sidebar-opacity",
                    |p| p.appearance.sidebar_opacity,
                    |p, v| p.appearance.sidebar_opacity = v,
                )}
                {font}
                text(class = "gds-settings-appearance__sample") {{move || i18n::text("font-sample")}}
            }
        })
    }
}

fn source_kind_key(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::System => "system",
        SourceKind::Builtin => "builtin",
        SourceKind::Custom => "custom",
    }
}

fn parse_source_kind(key: &str) -> SourceKind {
    match key {
        "system" => SourceKind::System,
        "custom" => SourceKind::Custom,
        _ => SourceKind::Builtin,
    }
}
