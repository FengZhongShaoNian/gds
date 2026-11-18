use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::Toggled;
use zgui::{component, view};
use zgui_ui::{ButtonProps as ZuiButtonProps, ButtonSize, ButtonVariant};

use crate::ui::Shortcut;
use crate::ui::icon::{IconName, IconProps};

/// Shared icon-only control wrapping `zgui_ui::Button` with accessible name and tooltip.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn IconButton(
    #[prop(into)] icon: Signal<IconName, LocalStorage>,
    #[prop(into)] label: Signal<String, LocalStorage>,
    #[prop(default = ButtonVariant::Ghost)] variant: ButtonVariant,
    #[prop(default = ButtonSize::Icon)] size: ButtonSize,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
    #[prop(into, default = Signal::stored_local(false))] toggled: Signal<bool, LocalStorage>,
    #[prop(optional)] shortcut: Option<Shortcut>,
    #[prop(default = None)] keytip: Option<&'static str>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    let _ = (shortcut, keytip);
    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-toggled"), move || {
            toggled.get().then(|| "true".to_owned())
        })
        .a11y_from(
            A11yBinding::new(Role::Button)
                .label(move || label.get())
                .toggled(move || {
                    if toggled.get() {
                        Toggled::True
                    } else {
                        Toggled::False
                    }
                }),
        );

    view! {
        ZuiButton(
            variant = variant,
            size = size,
            disabled = disabled,
            node_ref = node_ref,
            class = "gds-icon-button",
            class = class,
            {..own},
            {..attrs}
        ) {
            Icon(name = icon)
        }
    }
}
