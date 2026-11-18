use zgui::prelude::*;
use zgui::reactive::{Mounted, install};
use zgui::view;
use zgui::vocab::{Role, Toggled};

use super::{ButtonProps, ButtonSize, ButtonVariant, IconButton, IconButtonProps, apply_toggled};
use crate::ui::icon::IconName;

#[test]
fn button_variants_reexport_zgui_ui_types() {
    let _ = (ButtonVariant::Default, ButtonSize::Icon, IconButton);
}

#[test]
fn untoggled_buttons_omit_a11y_toggled_while_toggled_buttons_set_it() {
    install().ok();
    let scope = Mounted::new();
    scope.with(|| {
        let push = apply_toggled(Attrs::new().a11y_from(A11yBinding::new(Role::Button)), None);
        assert_eq!(
            push.a11y().map(|a| {
                let lowered = a.lower();
                (lowered.role, lowered.toggled)
            }),
            Some((Role::Button, None))
        );

        let toggled_on = apply_toggled(
            Attrs::new().a11y_from(A11yBinding::new(Role::Button)),
            Some(Signal::stored_local(true)),
        );
        assert_eq!(
            toggled_on.a11y().map(|a| {
                let lowered = a.lower();
                (lowered.role, lowered.toggled)
            }),
            Some((Role::Button, Some(Toggled::True)))
        );

        let _push_view = view! { Button { "Cancel" } };
        let _toggle_view = view! { Button(toggled = true) { "Mute" } };
        let _icon_push = view! { IconButton(icon = IconName::Menu, label = "Menu") };
        let _icon_toggle =
            view! { IconButton(icon = IconName::Pin, label = "Pin", toggled = false) };
    });
    scope.unmount();
}
