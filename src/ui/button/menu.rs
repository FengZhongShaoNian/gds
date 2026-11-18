use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::PropValue;
use zgui::{component, view};
use zgui_ui::{ButtonProps as ZuiButtonProps, ButtonSize, ButtonVariant};

use crate::ui::Shortcut;
use crate::ui::about::Application;
use crate::ui::icon::{IconName, IconProps};

/// Leading toolbar button that anchors the primary application menu.
///
/// Displays the application's monochrome brand mark at rest and crossfades to the
/// hamburger menu icon on hover or keyboard focus.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn ApplicationMenuButton(
    #[prop(into)] label: Signal<String, LocalStorage>,
    #[prop(optional)] application: Option<&'static Application>,
    #[prop(optional)] shortcut: Option<Shortcut>,
    #[prop(default = None)] keytip: Option<&'static str>,
    #[prop(into, default = Signal::stored_local(false))] active: Signal<bool, LocalStorage>,
    #[prop(optional)] on_activate: Option<UnsyncCallback<()>>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    let resolved_ref = node_ref.unwrap_or_default();
    let disabled = Signal::stored_local(false);
    if let Some(chord) = keytip {
        super::register_button_keytip(chord, resolved_ref, disabled, on_activate);
    }

    let own = super::button_attrs(
        Attrs::new().a11y_from(A11yBinding::unspecified().label(move || label.get())),
        None,
        Signal::stored_local(false),
        active,
        shortcut.as_ref(),
    );

    let brand_svg = application
        .and_then(Application::artwork_svg)
        .or_else(|| Application::try_cargo("gds-files").and_then(Application::artwork_svg));

    let core = AnyView::new(view! {
        ZuiButton(
            variant = ButtonVariant::Ghost,
            size = ButtonSize::Icon,
            node_ref = Some(resolved_ref),
            class = "gds-icon-button gds-app-menu-button",
            class = class,
            {..own},
            {..attrs}
        ) {
            box(class = "gds-app-menu-button__artwork") {
                {match brand_svg {
                    Some(markup) => AnyView::new(view! {
                        vector(
                            class = "gds-app-menu-button__brand",
                            prop:svg = PropValue::from(markup),
                        )
                    }),
                    None => AnyView::new(view! {
                        Icon(
                            name = IconName::Files,
                            size = crate::appearance::tokens::APPLICATION_MENU_ICON_SIZE,
                            class = "gds-app-menu-button__brand",
                        )
                    }),
                }}
                Icon(
                    name = IconName::Menu,
                    size = crate::appearance::tokens::ICON_SIZE,
                    class = "gds-app-menu-button__glyph",
                )
            }
        }
    });

    super::wrap_tooltip(core, Some(label), shortcut)
}
