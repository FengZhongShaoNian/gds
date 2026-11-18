use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

use super::IconButtonProps;
use crate::ui::icon::IconName;

/// Leading toolbar button that anchors the primary application menu.
#[component]
pub fn ApplicationMenuButton(
    #[prop(into)] label: Signal<String, LocalStorage>,
    #[prop(default = None)] keytip: Option<&'static str>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    view! {
        IconButton(
            icon = IconName::Menu,
            label = label,
            keytip = keytip,
            node_ref = node_ref,
            class = "gds-app-menu-button",
            {..attrs}
        )
    }
}
