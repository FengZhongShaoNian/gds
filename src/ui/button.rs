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
use zgui::vocab::Toggled;
use zgui::{component, view};
use zgui_ui::{
    ButtonProps as ZuiButtonProps, TooltipContentProps, TooltipProps, TooltipTriggerProps,
};

use crate::ui::Shortcut;

/// Standard GDS button wrapping `zgui_ui::Button` with optional tooltip/shortcut hint.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn Button(
    #[prop(default = ButtonVariant::Default)] variant: ButtonVariant,
    #[prop(default = ButtonSize::Md)] size: ButtonSize,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
    #[prop(into, default = Signal::stored_local(false))] toggled: Signal<bool, LocalStorage>,
    #[prop(into, optional)] tooltip: Option<String>,
    #[prop(optional)] shortcut: Option<Shortcut>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-toggled"), move || {
            toggled.get().then(|| "true".to_owned())
        })
        .a11y_from(A11yBinding::new(Role::Button).toggled(move || {
            if toggled.get() {
                Toggled::True
            } else {
                Toggled::False
            }
        }));

    let core = view! {
        ZuiButton(
            variant = variant,
            size = size,
            disabled = disabled,
            node_ref = node_ref,
            class = class,
            {..own},
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    };

    if let Some(tip) = tooltip {
        let hint = Signal::stored_local(
            shortcut
                .map(|key| format!("{tip} ({})", key.display_tokens().join("+")))
                .unwrap_or(tip),
        );
        AnyView::new(view! {
            Tooltip {
                TooltipTrigger {{core}}
                TooltipContent {{move || hint.get()}}
            }
        })
        .into_view()
    } else {
        AnyView::new(core).into_view()
    }
}
