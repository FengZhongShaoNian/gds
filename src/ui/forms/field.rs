//! Unified GDS input pills, field labels, and searchable choice controls.

pub mod placeholder;
pub use placeholder::Placeholder;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, UnsyncCallback};
use zgui::view::{AttrName, ShowProps};
use zgui::vocab::{PointerButton, Role, UiState};
use zgui::{component, css, view};
use zgui_ui::listbox::{Listbox, ListboxCatalogue, ListboxOption, use_listbox_option};
use zgui_ui::{
    ComboboxContentProps, ComboboxEmptyProps, ComboboxInputProps, ComboboxProps, InputProps,
    TextareaProps,
};
use zgui_ui_primitives::Binding;

use crate::app::Gds;
use crate::i18n::Language;
use crate::ui::icon::{IconName, IconProps};

const FIELD_SHEET: &str = "gds-ui-field";

const FIELD_CSS: &str = css!(
    ".gds-input-field.gds-input-field {
        position: relative;
        display: flex;
        flex-direction: row;
        align-items: center;
        width: 100%;
        min-width: 0px;
        height: var(--gds-control-height, 36px);
        min-height: var(--gds-control-height, 36px);
        padding: 0px 10px;
        box-sizing: border-box;
        background: var(--gds-content);
        color: var(--gds-foreground);
        border: 1px solid var(--gds-outline);
        border-radius: var(--gds-radius-control, 6px);
        box-shadow: none;
        outline: none;
        transition: border-color var(--gds-motion-fast, 100ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }
    .gds-input-field.gds-input-field[data-has-leading=\"true\"] {
        padding-left: 0px;
    }
    .gds-input-field.gds-input-field[data-has-trailing=\"true\"] {
        padding-right: 0px;
    }
    .gds-input-field:not([data-has-trailing=\"true\"]) > .gds-input-field__trailing {
        display: none;
    }
    .gds-input-field.gds-input-field[data-compact=\"true\"] {
        height: var(--gds-icon-button-size, 32px);
        min-height: var(--gds-icon-button-size, 32px);
    }
    .gds-input-field.gds-input-field:focus-within {
        border-color: var(--gds-primary);
        box-shadow: none;
        outline: none;
    }
    .gds-input-field.gds-input-field[data-invalid=\"true\"],
    .gds-input-field.gds-input-field[data-invalid=\"true\"]:focus-within {
        border-color: var(--gds-error);
        box-shadow: none;
        outline: none;
    }
    .gds-input-field.gds-input-field[data-disabled=\"true\"] {
        opacity: 0.5;
        pointer-events: none;
    }
    .gds-input-field__leading {
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        gap: 4px;
        padding-left: 10px;
        color: var(--gds-muted-foreground);
        cursor: text;
        user-select: none;
    }
    .gds-input-field__trailing {
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        gap: 4px;
        padding-right: 4px;
        color: var(--gds-muted-foreground);
    }
    .gds-input-field__clear {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 24px;
        height: 24px;
        flex-shrink: 0;
        padding: 0px;
        border: none;
        border-radius: 4px;
        background: transparent;
        color: var(--gds-muted-foreground);
        cursor: pointer;
        outline: none;
        box-shadow: none;
    }
    .gds-input-field__clear:hover {
        background: var(--gds-hover);
        color: var(--gds-foreground);
    }
    .gds-input-field__clear:active {
        background: var(--gds-pressed);
    }
    .gds-input-field__clear:focus-visible {
        outline: 1px solid var(--gds-primary);
        outline-offset: -1px;
    }
    .gds-input-field .zui-field.gds-input-field__input {
        flex: 1 1 0%;
        width: auto;
        height: 100%;
        min-width: 0px;
        padding: 0px;
        border: 0px solid transparent;
        border-radius: 0px;
        background: transparent;
        color: var(--gds-foreground);
        font-family: var(--gds-font-sans);
        font-size: var(--gds-font-size-small, var(--gds-font-size-sm, 14px));
        box-shadow: none;
        outline: none;
    }
    .gds-input-field[data-has-leading=\"true\"] .zui-field.gds-input-field__input {
        padding-left: 6px;
    }
    .gds-input-field[data-has-trailing=\"true\"] .zui-field.gds-input-field__input {
        padding-right: 4px;
    }
    .gds-input-field .zui-field.gds-input-field__input:focus,
    .gds-input-field .zui-field.gds-input-field__input:focus-visible,
    .gds-input-field .zui-field.gds-input-field__input:invalid,
    .gds-input-field .zui-field.gds-input-field__input:invalid:focus,
    .gds-input-field .zui-field.gds-input-field__input:invalid:focus-visible {
        border-color: transparent;
        box-shadow: none;
        outline: none;
    }
    .gds-input-field .zui-field.gds-input-field__input:empty::before {
        color: var(--gds-muted-foreground);
    }
    .zui-field.gds-textarea-field {
        background: var(--gds-content);
        color: var(--gds-foreground);
        border: 1px solid var(--gds-outline);
        border-radius: var(--gds-radius-control, 6px);
        font-family: var(--gds-font-sans);
        font-size: var(--gds-font-size-small, var(--gds-font-size-sm, 14px));
        box-shadow: none;
        outline: none;
    }
    .zui-field.gds-textarea-field:focus,
    .zui-field.gds-textarea-field:focus-visible {
        border-color: var(--gds-primary);
        box-shadow: none;
        outline: none;
    }
    .zui-field.gds-textarea-field:invalid,
    .zui-field.gds-textarea-field:invalid:focus,
    .zui-field.gds-textarea-field:invalid:focus-visible {
        border-color: var(--gds-error);
        box-shadow: none;
        outline: none;
    }
    .zui-input-group.gds-search-choice-field {
        min-height: var(--gds-control-height, 36px);
        background: var(--gds-content);
        color: var(--gds-foreground);
        border: 1px solid var(--gds-outline);
        border-radius: var(--gds-radius-control, 6px);
        box-shadow: none;
        outline: none;
    }
    .zui-input-group.gds-search-choice-field:focus-within {
        border-color: var(--gds-primary);
        box-shadow: none;
        outline: none;
    }"
);

pub(crate) fn install_field_stylesheets() {
    crate::ui::styles::install_ui_stylesheets();
    zgui::view::install_stylesheet(FIELD_SHEET, FIELD_CSS);
}

fn clear_button_label() -> String {
    Gds::try_current().map_or_else(
        || "Clear".to_owned(),
        |gds| match gds.i18n().language() {
            Language::Chinese => "清空".to_owned(),
            Language::English => "Clear".to_owned(),
        },
    )
}

/// Accessible field title label bound with `Role::Label` and optional `node_ref` for `labelled_by`.
#[component]
pub fn FieldLabel(
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    install_field_stylesheets();
    view! {
        text(
            class = "gds-setting-title",
            a11y:role = Role::Label,
            node_ref = node_ref.unwrap_or_default(),
            class = class,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}

/// Option entry for [`SearchChoiceField`], supporting display labels plus hidden search keywords.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchOption {
    pub value: String,
    pub label: String,
    pub keywords: Vec<String>,
}

impl SearchOption {
    #[must_use]
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            keywords: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn matches_query(&self, query: &str) -> bool {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return true;
        }
        let label_lower = self.label.to_lowercase();
        let value_lower = self.value.to_lowercase();
        let keyword_lowers: Vec<String> = self
            .keywords
            .iter()
            .map(|term| term.to_lowercase())
            .collect();
        trimmed.split_whitespace().all(|token| {
            let needle = token.to_lowercase();
            label_lower.contains(&needle)
                || value_lower.contains(&needle)
                || keyword_lowers.iter().any(|term| term.contains(&needle))
        })
    }
}

#[component]
fn SearchChoiceEntry(
    value: String,
    label: String,
    disabled: Signal<bool, LocalStorage>,
) -> impl IntoView {
    let node = NodeRef::new();
    let registered = use_listbox_option(ListboxOption::new(
        node,
        value.clone(),
        label.clone(),
        disabled,
    ));
    let value_store = zgui::reactive::StoredValue::new_local(value);
    let chosen =
        move || registered.is_some_and(|(listbox, _)| listbox.is_chosen(&value_store.get_value()));
    let active = move || registered.is_some_and(|(listbox, id)| listbox.active() == Some(id));

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-select__item"), true)
        .attribute(AttrName::new("data-value"), move || {
            Some(value_store.get_value())
        })
        .attribute(AttrName::new("data-state"), move || {
            Some(if chosen() { "checked" } else { "unchecked" }.to_owned())
        })
        .attribute(AttrName::new("data-active"), move || {
            active().then(|| "true".to_owned())
        })
        .attribute(AttrName::new("data-disabled"), move || {
            disabled.get().then(|| "true".to_owned())
        })
        .state(UiState::CHECKED, chosen)
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(
            A11yBinding::new(Role::ListBoxOption)
                .selected(chosen)
                .disabled(move || disabled.get()),
        );

    view! {
        box(
            node_ref = node,
            on:pointer_down = |ev: &mut EventCx<'_, events::PointerDown>| {
                if ev.button == Some(PointerButton::Primary) {
                    ev.prevent_default();
                    ev.activate_now();
                }
            },
            on:click = move |_| {
                if let Some((listbox, id)) = registered {
                    listbox.take(id);
                }
            },
            on:pointer_enter = move |_| {
                if !disabled.get_untracked()
                    && let Some((listbox, id)) = registered
                {
                    listbox.set_active(Some(id));
                }
            },
            {..own}
        ) {
            box(class = "zui-select__indicator") {
                Show(when = chosen, fallback = || ()) {
                    Icon(name = IconName::Check, size = 14.0)
                }
            }
            text {{label}}
        }
    }
}

#[component]
fn SearchChoiceOption(option: SearchOption, disabled: Signal<bool, LocalStorage>) -> impl IntoView {
    let listbox = Listbox::current();
    if let Some(listbox) = listbox
        && ListboxCatalogue::is_current()
    {
        listbox.remember(&option.value, &option.label);
        return AnyView::new(());
    }

    let matcher = option.clone();
    let survives =
        move || listbox.is_none_or(|lb| !lb.query_active() || matcher.matches_query(&lb.filter()));
    let held = zgui::reactive::StoredValue::new_local(option);

    AnyView::new(view! {
        Show(when = survives, fallback = || ()) {
            {move || {
                let item = held.get_value();
                view! {
                    SearchChoiceEntry(
                        value = item.value,
                        label = item.label,
                        disabled = disabled,
                    )
                }
            }}
        }
    })
}

/// Searchable choice combobox supporting matching on both visible labels and hidden keywords.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn SearchChoiceField(
    #[prop(into, optional)] value: Binding<String>,
    #[prop(into)] options: Signal<Vec<SearchOption>, LocalStorage>,
    #[prop(into, optional)] placeholder: Option<String>,
    #[prop(into, default = Signal::stored_local("No matches".to_owned()))] empty_text: Signal<
        String,
        LocalStorage,
    >,
    #[prop(optional)] on_change: Option<UnsyncCallback<String>>,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
    #[prop(into, optional)] label: Option<String>,
    #[prop(optional)] labelled_by: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    install_field_stylesheets();
    view! {
        Combobox(
            value = value,
            on_change = on_change,
        ) {
            ComboboxInput(
                placeholder = placeholder,
                disabled = disabled,
                label = label,
                labelled_by = labelled_by,
                class = "gds-search-choice-field",
                class = class,
                {..attrs}
            ) {}
            ComboboxContent {
                for option in move || options.get(), key = |opt| (opt.value.clone(), opt.label.clone()) {
                    SearchChoiceOption(option = option, disabled = disabled)
                }
                ComboboxEmpty {{move || empty_text.get()}}
            }
        }
    }
}

/// Unified `6px`-radius GDS single-line input pill with optional leading icon/view,
/// trailing view, `32px` compact mode, and inline clear button.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn InputField(
    #[prop(into, optional)] value: Binding<String>,
    #[prop(into, optional)] default_value: Option<String>,
    #[prop(into, optional)] placeholder: Option<String>,
    #[prop(optional)] on_change: Option<UnsyncCallback<String>>,
    #[prop(optional)] on_clear: Option<UnsyncCallback<()>>,
    #[prop(optional)] leading_icon: Option<IconName>,
    #[prop(optional)] leading: Option<AnyView>,
    #[prop(optional)] trailing: Option<AnyView>,
    #[prop(default = false)] clearable: bool,
    #[prop(default = false)] compact: bool,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
    #[prop(into, default = Signal::stored_local(false))] read_only: Signal<bool, LocalStorage>,
    #[prop(into, default = Signal::stored_local(false))] required: Signal<bool, LocalStorage>,
    #[prop(into, default = Signal::stored_local(false))] invalid: Signal<bool, LocalStorage>,
    #[prop(into, optional)] secure: Option<Signal<bool, LocalStorage>>,
    #[prop(into, optional)] label: Option<String>,
    #[prop(optional)] labelled_by: Option<NodeRef>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    install_field_stylesheets();
    let field_ref = node_ref.unwrap_or_default();
    let unbound_non_empty =
        RwSignal::new_local(default_value.as_ref().is_some_and(|s| !s.is_empty()));

    let combined_on_change = UnsyncCallback::new(move |next: String| {
        if !value.is_bound() {
            unbound_non_empty.set(!next.is_empty());
        }
        if let Some(cb) = on_change {
            cb.run(next);
        }
    });

    let show_clear = Signal::derive_local(move || {
        if !clearable || disabled.get() || read_only.get() {
            return false;
        }
        value
            .get()
            .map_or_else(|| unbound_non_empty.get(), |text| !text.is_empty())
    });

    let has_leading = leading_icon.is_some() || leading.is_some();
    let has_static_trailing = trailing.is_some();
    let has_trailing = move || has_static_trailing || show_clear.get();

    let leading_slot = has_leading.then(|| {
        AnyView::new(view! {
            row(
                class = "gds-input-field__leading",
                a11y:hidden = true,
                on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                    if ev.button == Some(PointerButton::Primary) && !disabled.get_untracked() {
                        ev.prevent_default();
                        field_ref.focus();
                    }
                },
            ) {
                {leading_icon.map(|icon| view! { Icon(name = icon, size = 16.0) }.into_view())}
                {leading}
            }
        })
    });

    let clear_view = clearable.then(|| {
        let on_clear_click = move |_: &mut EventCx<'_, events::Click>| {
            if disabled.get_untracked() || read_only.get_untracked() {
                return;
            }
            if value.is_bound() {
                value.write(String::new());
            } else {
                field_ref.set_value("");
                unbound_non_empty.set(false);
            }
            if let Some(cb) = on_change {
                cb.run(String::new());
            }
            if let Some(cb) = on_clear {
                cb.run(());
            }
            field_ref.focus();
        };
        AnyView::new(view! {
            Show(when = move || show_clear.get(), fallback = || ()) {
                control(
                    class = "gds-input-field__clear",
                    a11y:role = Role::Button,
                    a11y:label = clear_button_label,
                    on:pointer_down = |ev: &mut EventCx<'_, events::PointerDown>| {
                        if ev.button == Some(PointerButton::Primary) {
                            ev.prevent_default();
                            ev.activate_now();
                        }
                    },
                    on:click = on_clear_click,
                ) {
                    Icon(name = IconName::X, size = 14.0)
                }
            }
        })
    });

    let trailing_slot = (clearable || has_static_trailing).then(|| {
        AnyView::new(view! {
            row(class = "gds-input-field__trailing") {
                {clear_view}
                {trailing}
            }
        })
    });

    let own = Attrs::new()
        .attribute(
            AttrName::new("data-compact"),
            compact.then(|| "true".to_owned()),
        )
        .attribute(
            AttrName::new("data-has-leading"),
            has_leading.then(|| "true".to_owned()),
        )
        .attribute(AttrName::new("data-has-trailing"), move || {
            has_trailing().then(|| "true".to_owned())
        })
        .attribute(AttrName::new("data-disabled"), move || {
            disabled.get().then(|| "true".to_owned())
        })
        .attribute(AttrName::new("data-invalid"), move || {
            invalid.get().then(|| "true".to_owned())
        });

    view! {
        row(
            class = "gds-input-field",
            {..own},
            {..attrs},
            class = class,
        ) {
            {leading_slot}
            Input(
                value = value,
                default_value = default_value,
                placeholder = placeholder,
                on_change = combined_on_change,
                disabled = disabled,
                read_only = read_only,
                required = required,
                invalid = invalid,
                secure = secure,
                label = label,
                labelled_by = labelled_by,
                node_ref = field_ref,
                class = "gds-input-field__input",
            ) {}
            {trailing_slot}
        }
    }
}

/// Multi-line GDS input editor delegating to `zgui_ui::Textarea`.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn TextareaField(
    #[prop(into, optional)] value: Binding<String>,
    #[prop(into, optional)] placeholder: Option<String>,
    #[prop(optional)] on_change: Option<UnsyncCallback<String>>,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
    #[prop(into, default = Signal::stored_local(false))] invalid: Signal<bool, LocalStorage>,
    #[prop(into, optional)] label: Option<String>,
    #[prop(optional)] labelled_by: Option<NodeRef>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    install_field_stylesheets();
    view! {
        Textarea(
            value = value,
            placeholder = placeholder,
            on_change = on_change,
            disabled = disabled,
            invalid = invalid,
            label = label,
            labelled_by = labelled_by,
            node_ref = node_ref,
            class = "gds-textarea-field",
            class = class,
            {..attrs}
        )
    }
}
