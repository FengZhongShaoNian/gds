//! Thin GDS wrappers around `zgui-ui` input, label, and searchable choice controls.

pub mod placeholder;
pub use placeholder::Placeholder;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::view::{AttrName, ShowProps};
use zgui::vocab::{PointerButton, Role, UiState};
use zgui::{component, view};
use zgui_ui::listbox::{Listbox, ListboxCatalogue, ListboxOption, use_listbox_option};
use zgui_ui::{
    ComboboxContentProps, ComboboxEmptyProps, ComboboxInputProps, ComboboxProps, InputProps,
    TextareaProps,
};
use zgui_ui_primitives::Binding;

use crate::ui::icon::{IconName, IconProps};

/// Accessible field title label bound with `Role::Label` and optional `node_ref` for `labelled_by`.
#[component]
pub fn FieldLabel(
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
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
    crate::ui::styles::install_ui_stylesheets();
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

/// Single-line GDS input field delegating to `zgui_ui::Input`.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn InputField(
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
    crate::ui::styles::install_ui_stylesheets();
    view! {
        Input(
            value = value,
            placeholder = placeholder,
            on_change = on_change,
            disabled = disabled,
            invalid = invalid,
            label = label,
            labelled_by = labelled_by,
            node_ref = node_ref,
            class = class,
            {..attrs}
        )
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
    crate::ui::styles::install_ui_stylesheets();
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
            class = class,
            {..attrs}
        )
    }
}
