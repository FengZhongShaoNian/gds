use std::rc::Rc;

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, UnsyncCallback};
use zgui::{component, view};
use zgui_ui::{
    ComboboxContentProps, ComboboxEmptyProps, ComboboxInputProps, ComboboxItemProps, ComboboxProps,
    InputProps, SelectContentProps, SelectItemProps, SelectProps, SelectTriggerProps,
    SelectValueProps, SwitchProps, TextareaProps,
};
use zgui_ui_primitives::{Binding, Controllable};

use super::{Choice, Field, Kind, Model};
use crate::app::Gds;
use crate::ui::button::IconButtonProps;
use crate::ui::icon::IconName;
use crate::ui::settings::{
    NumberControlProps, SettingActionsProps, SettingControlProps, SettingDescriptionProps,
    SettingLabelProps, SettingResetProps, SettingRowProps,
};

/// Emitted whenever a validated settings form edit produces a new model value.
#[derive(Clone, Debug, PartialEq)]
pub struct Changed<T> {
    pub value: T,
}

/// Declarative settings form view derived for any [`Model`].
pub struct Editor<T: Model> {
    value: Binding<T>,
    on_change: Option<UnsyncCallback<Changed<T>>>,
    disabled: Signal<bool, LocalStorage>,
    reset_focus: Option<NodeRef>,
}

impl<T: Model> Editor<T> {
    #[must_use]
    pub fn new(value: impl Into<Binding<T>>) -> Self {
        Self {
            value: value.into(),
            on_change: None,
            disabled: Signal::stored_local(false),
            reset_focus: None,
        }
    }

    #[must_use]
    pub fn on_change(mut self, callback: impl Fn(Changed<T>) + 'static) -> Self {
        self.on_change = Some(UnsyncCallback::new(callback));
        self
    }

    #[must_use]
    pub fn disabled(mut self, disabled: impl Into<Signal<bool, LocalStorage>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    #[must_use]
    pub fn reset_focus(mut self, focus: NodeRef) -> Self {
        self.reset_focus = Some(focus);
        self
    }

    /// Build the settings editor view tree.
    pub fn view(self) -> AnyView
    where
        T: Serialize + DeserializeOwned,
    {
        let on_change = self.on_change;
        let forward =
            on_change.map(|cb| UnsyncCallback::new(move |value: T| cb.run(Changed { value })));
        let state = Controllable::new(self.value, T::default(), forward);
        AnyView::new(view! {
            EditorBody(
                state = state,
                disabled = self.disabled,
                reset_focus = self.reset_focus,
            )
        })
    }
}

impl<T: Model + Serialize + DeserializeOwned> IntoView for Editor<T> {
    type Output = AnyView;

    fn into_view(self) -> Self::Output {
        self.view()
    }
}

#[component]
fn EditorBody<T: Model + Serialize + DeserializeOwned>(
    state: Controllable<T>,
    disabled: Signal<bool, LocalStorage>,
    reset_focus: Option<NodeRef>,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    let defaults = Rc::new(T::default());
    let mut rows: Vec<AnyView> = Vec::new();
    let mut active_group = "";
    for field in T::fields() {
        if field.group != active_group {
            active_group = field.group;
            if !active_group.is_empty() {
                let group = active_group;
                rows.push(AnyView::new(view! {
                    text(class = "gds-setting-group") {{move || Gds::current().i18n().text(group)}}
                }));
            }
        }
        rows.push(AnyView::new(view! {
            EditorFieldRow(
                state = state,
                field = Rc::new(field),
                defaults = Rc::clone(&defaults),
                disabled = disabled,
                reset_focus = reset_focus,
            )
        }));
    }

    view! {
        column(class = "gds-setting-form") {
            {rows}
        }
    }
}

#[component]
fn EditorFieldRow<T: Model + Serialize + DeserializeOwned>(
    state: Controllable<T>,
    field: Rc<Field<T>>,
    defaults: Rc<T>,
    disabled: Signal<bool, LocalStorage>,
    reset_focus: Option<NodeRef>,
) -> impl IntoView {
    let error: RwSignal<Option<String>, LocalStorage> = RwSignal::new_local(None);
    let label_key = field.label;
    let help_key = field.help;

    let apply_json = {
        let field = Rc::clone(&field);
        move |json: Value| {
            let mut next = state.get_untracked();
            if let Err(err) = (field.set)(&mut next, json) {
                error.set(Some(err));
                return;
            }
            if let Err(err) = next.validate() {
                error.set(Some(err));
                return;
            }
            error.set(None);
            state.set(next);
        }
    };

    let modified = {
        let field = Rc::clone(&field);
        let defaults = Rc::clone(&defaults);
        Signal::derive_local(move || state.get().is_modified(&defaults, &field))
    };

    let reset = {
        let field = Rc::clone(&field);
        let defaults = Rc::clone(&defaults);
        move |_: &mut zgui::view::EventCx<'_, zgui::view::events::Click>| {
            let default_json = (field.get)(&defaults);
            let mut next = state.get_untracked();
            if (field.set)(&mut next, default_json).is_ok() && next.validate().is_ok() {
                error.set(None);
                state.set(next);
            }
            if let Some(target) = reset_focus {
                target.focus();
            }
        }
    };

    let control_view = render_field_control(state, &field, disabled, error, apply_json);

    view! {
        SettingRow {
            SettingLabel {
                text(class = "gds-setting-title") {{move || Gds::current().i18n().text(label_key)}}
                if move || !help_key.is_empty() {
                    SettingDescription {{move || Gds::current().i18n().text(help_key)}}
                } else {}
                {move || error.get().map(|msg| view! {
                    text(class = "gds-setting-error") {{msg}}
                })}
            }
            SettingActions {
                SettingControl {
                    {control_view}
                }
                SettingReset {
                    if move || modified.get() {
                        IconButton(
                            icon = IconName::RotateCcw,
                            label = Signal::derive_local(|| Gds::current().i18n().text("form-reset")),
                            disabled = disabled,
                            on:click = reset.clone(),
                        )
                    } else {}
                }
            }
        }
    }
}

trait FieldModifiedExt<T> {
    fn is_modified(&self, defaults: &T, field: &Field<T>) -> bool;
}

impl<T> FieldModifiedExt<T> for T {
    fn is_modified(&self, defaults: &T, field: &Field<T>) -> bool {
        field.is_modified(self, defaults)
    }
}

fn render_field_control<T: Model + Serialize + DeserializeOwned>(
    state: Controllable<T>,
    field: &Rc<Field<T>>,
    disabled: Signal<bool, LocalStorage>,
    error: RwSignal<Option<String>, LocalStorage>,
    apply_json: impl Fn(Value) + Clone + 'static,
) -> AnyView {
    let field_id = field.id;
    let label_key = field.label;
    match field.kind {
        Kind::Toggle => {
            let field = Rc::clone(field);
            let binding = Binding::controlled(
                Signal::derive_local(move || {
                    field.effective(&state.get()).as_bool().unwrap_or(false)
                }),
                move |next: bool| apply_json(Value::Bool(next)),
            );
            AnyView::new(view! { Switch(checked = binding, disabled = disabled) })
        }
        Kind::Number(number) => {
            let range = number.range();
            let field = Rc::clone(field);
            let read_display = Signal::derive_local(move || {
                let raw = field.effective(&state.get()).as_f64().unwrap_or(number.min);
                f64::from(range.quantize((raw * number.scale) as f32))
            });
            let apply_for_slider = apply_json.clone();
            let binding = Binding::controlled(read_display, move |display: f64| {
                match number.value(display) {
                    Ok(val) => apply_for_slider(val),
                    Err(msg) => error.set(Some(msg)),
                }
            });
            AnyView::new(view! {
                row(class = "gds-setting-number-row") {
                    NumberControl(
                        value = binding,
                        range = range,
                        disabled = disabled,
                        label = Gds::current().i18n().text(label_key),
                    )
                    text(class = "gds-setting-number-value") {{move || number.label(read_display.get())}}
                }
            })
        }
        Kind::Text => {
            let field = Rc::clone(field);
            let nullable = field.nullable;
            let binding = Binding::controlled(
                Signal::derive_local(move || {
                    (field.get)(&state.get())
                        .as_str()
                        .map_or_else(String::new, str::to_owned)
                }),
                move |text: String| {
                    let val = if nullable && text.trim().is_empty() {
                        Value::Null
                    } else {
                        Value::String(text)
                    };
                    apply_json(val);
                },
            );
            let invalid = Signal::derive_local(move || error.get().is_some());
            AnyView::new(view! {
                Input(
                    value = binding,
                    disabled = disabled,
                    invalid = invalid,
                    label = Gds::current().i18n().text(label_key),
                )
            })
        }
        Kind::StringList => {
            let field = Rc::clone(field);
            let binding = Binding::controlled(
                Signal::derive_local(move || {
                    (field.get)(&state.get())
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default()
                }),
                move |text: String| {
                    let list: Vec<Value> = text
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(|line| Value::String(line.to_owned()))
                        .collect();
                    apply_json(Value::Array(list));
                },
            );
            let invalid = Signal::derive_local(move || error.get().is_some());
            AnyView::new(view! {
                Textarea(
                    value = binding,
                    disabled = disabled,
                    invalid = invalid,
                    label = Gds::current().i18n().text(label_key),
                )
            })
        }
        Kind::Font | Kind::Language | Kind::Choice(_) => {
            let field_read = Rc::clone(field);
            let field_choices = Rc::clone(field);
            let choices = field_choices.choices(&state.get_untracked());
            let value_binding = Binding::controlled(
                Signal::derive_local(move || (field_read.get)(&state.get()).to_string()),
                move |encoded: String| {
                    if let Ok(json) = serde_json::from_str::<Value>(&encoded) {
                        apply_json(json);
                    }
                },
            );
            if matches!(field.kind, Kind::Font) {
                let list = Signal::stored_local(choices);
                let placeholder = Gds::current().i18n().text(label_key);
                let empty_label = Signal::stored_local(format!("No options for {field_id}"));
                AnyView::new(view! {
                    Combobox(value = value_binding) {
                        ComboboxInput(placeholder = placeholder.clone(), disabled = disabled, label = placeholder) {}
                        ComboboxContent {
                            for choice in move || list.get(), key = |c| c.value.to_string() {
                                {combobox_choice_item(choice)}
                            }
                            ComboboxEmpty {{move || empty_label.get()}}
                        }
                    }
                })
            } else {
                let list = Signal::stored_local(choices);
                let label_text = Gds::current().i18n().text(label_key);
                AnyView::new(view! {
                    Select(value = value_binding) {
                        SelectTrigger(disabled = disabled, label = label_text) {
                            SelectValue()
                        }
                        SelectContent {
                            for choice in move || list.get(), key = |c| c.value.to_string() {
                                {select_choice_item(choice)}
                            }
                        }
                    }
                })
            }
        }
    }
}

fn combobox_choice_item(choice: Choice) -> AnyView {
    let encoded = choice.value.to_string();
    let title = choice.title;
    let title_sig = Signal::stored_local(title.clone());
    AnyView::new(view! {
        ComboboxItem(value = encoded, text = title) {
            {move || title_sig.get()}
        }
    })
}

fn select_choice_item(choice: Choice) -> AnyView {
    let encoded = choice.value.to_string();
    let title = choice.title;
    let title_sig = Signal::stored_local(title.clone());
    AnyView::new(view! {
        SelectItem(value = encoded, text = title) {
            {move || title_sig.get()}
        }
    })
}
