use super::{ButtonSize, ButtonVariant, IconButton};

#[test]
fn button_variants_reexport_zgui_ui_types() {
    let _ = (ButtonVariant::Default, ButtonSize::Icon, IconButton);
}
