//! Thin GDS wrappers around `zgui-ui` input and choice controls.

pub mod placeholder;
pub use placeholder::Placeholder;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui::{InputProps, TextareaProps};
use zgui_ui_primitives::Binding;

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
            node_ref = node_ref,
            class = class,
            {..attrs}
        )
    }
}
