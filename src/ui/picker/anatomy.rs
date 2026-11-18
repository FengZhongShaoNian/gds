use zgui::prelude::*;
use zgui::{component, view};

/// A picker column; its host owns the title and decision footer.
#[component]
pub fn Picker(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    crate::ui::styles::install_ui_stylesheets();
    view! {
        column(class = "gds-picker", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}

/// Compact navigation or filter controls with fixed, shared spacing.
#[component]
pub fn PickerToolbar(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        row(class = "gds-picker__toolbar", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}

/// The flexible location slot, shared by breadcrumbs and path editing.
#[component]
pub fn PickerLocation(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        row(class = "gds-picker__location", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}

/// Scrollable picker result body.
#[component]
pub fn PickerContent(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        box(class = "gds-picker__content", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}

/// Status line below a picker listing.
#[component]
pub fn PickerStatus(
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    view! {
        row(class = "gds-picker__status", class = class, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
