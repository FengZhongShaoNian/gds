//! Shared GDS buttons wrapping `zgui-ui::Button` with shortcuts, keytips, and tooltips.

mod icon;
mod menu;
#[cfg(test)]
mod tests;

pub use icon::{IconButton, IconButtonProps};
pub use menu::{ApplicationMenuButton, ApplicationMenuButtonProps};
pub use zgui_ui::{ButtonSize, ButtonVariant};

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::AttrName;
use zgui::vocab::Toggled;
use zgui::{component, view};
use zgui_ui::{
    ButtonProps as ZuiButtonProps, SpinnerProps, TooltipContentProps, TooltipProps,
    TooltipTriggerProps,
};

use crate::ui::keytips::register_keytip;
use crate::ui::tooltip::{TOOLTIP_DELAY, TooltipTextProps};
use crate::ui::{Shortcut, ShortcutBadgeProps};

pub(super) fn apply_toggled(attrs: Attrs, toggled: Option<Signal<bool, LocalStorage>>) -> Attrs {
    let Some(toggled) = toggled else {
        return attrs;
    };
    attrs
        .attribute(AttrName::new("data-toggled"), move || {
            toggled.get().then(|| "true".to_owned())
        })
        .a11y_from(A11yBinding::unspecified().toggled(move || {
            if toggled.get() {
                Toggled::True
            } else {
                Toggled::False
            }
        }))
}

pub(super) fn button_attrs(
    base: Attrs,
    toggled: Option<Signal<bool, LocalStorage>>,
    loading: Signal<bool, LocalStorage>,
    active: Signal<bool, LocalStorage>,
    shortcut: Option<&Shortcut>,
) -> Attrs {
    let mut attrs = apply_toggled(base, toggled)
        .attribute(AttrName::new("data-loading"), move || {
            loading.get().then(|| "true".to_owned())
        })
        .attribute(AttrName::new("data-active"), move || {
            active.get().then(|| "true".to_owned())
        })
        .a11y_from(A11yBinding::unspecified().busy(move || loading.get()));
    if let Some(key) = shortcut {
        let hint = zgui::vocab::SharedString::from(key.display_tokens().join("+"));
        attrs = attrs.a11y_from(A11yBinding::unspecified().keyboard_shortcut(hint));
    }
    attrs
}

pub(super) fn register_button_keytip(
    chord: &'static str,
    target: NodeRef,
    disabled: Signal<bool, LocalStorage>,
    on_activate: Option<UnsyncCallback<()>>,
) {
    register_keytip(
        chord,
        target,
        UnsyncCallback::new(move |()| {
            if disabled.get_untracked() {
                return;
            }
            target.focus();
            if let Some(callback) = &on_activate {
                callback.run(());
            }
        }),
    );
}

pub(super) fn wrap_tooltip(
    core: AnyView,
    label: Option<Signal<String, LocalStorage>>,
    shortcut: Option<Shortcut>,
) -> AnyView {
    if label.is_none() && shortcut.is_none() {
        return core;
    }
    AnyView::new(view! {
        Tooltip(delay = TOOLTIP_DELAY) {
            TooltipTrigger {{core}}
            TooltipContent {
                row(class = "gds-shortcut-hint") {
                    {label.map(|text| view! { TooltipText(text = text) }.into_view())}
                    {shortcut
                        .as_ref()
                        .map(|key| view! { ShortcutBadge(shortcut = key.clone()) }.into_view())}
                }
            }
        }
    })
}

/// Standard GDS button wrapping `zgui_ui::Button` with optional tooltip, shortcut hint,
/// loading spinner, and mnemonic keytip registration.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn Button(
    #[prop(default = ButtonVariant::Default)] variant: ButtonVariant,
    #[prop(default = ButtonSize::Md)] size: ButtonSize,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
    #[prop(into, default = Signal::stored_local(false))] loading: Signal<bool, LocalStorage>,
    #[prop(into, default = Signal::stored_local(false))] active: Signal<bool, LocalStorage>,
    #[prop(into, optional)] toggled: Option<Signal<bool, LocalStorage>>,
    #[prop(into, optional)] tooltip: Option<String>,
    #[prop(optional)] shortcut: Option<Shortcut>,
    #[prop(default = None)] keytip: Option<&'static str>,
    #[prop(optional)] on_activate: Option<UnsyncCallback<()>>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    let resolved_ref = node_ref.unwrap_or_default();
    let effective_disabled = Signal::derive_local(move || disabled.get() || loading.get());
    if let Some(chord) = keytip {
        register_button_keytip(chord, resolved_ref, effective_disabled, on_activate);
    }

    let own = button_attrs(Attrs::new(), toggled, loading, active, shortcut.as_ref());
    let tip_signal = tooltip.map(Signal::stored_local);

    let core = AnyView::new(view! {
        ZuiButton(
            variant = variant,
            size = size,
            disabled = effective_disabled,
            node_ref = Some(resolved_ref),
            class = "gds-button",
            class = class,
            {..own},
            {..attrs}
        ) {
            {move || loading.get().then(|| view! { Spinner() }.into_view())}
            {children.into_view_once()}
        }
    });

    wrap_tooltip(core, tip_signal, shortcut)
}
