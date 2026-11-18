//! Keyboard focus regions and shared focus-ring styling helpers.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, RwSignal};
use zgui::view::{AttrName, current_host, events};
use zgui::vocab::Role;
use zgui::{component, css, view};

const SHEET: &str = css!(
    r#"
    .gds-region-surface {
        outline: 1px solid transparent;
        outline-offset: -1px;
    }

    .gds-region-surface[data-region-focused="true"] {
        border-color: var(--gds-border);
    }

    .gds-region-surface[data-focus-visible="true"] {
        outline: 2px solid var(--gds-outline);
        outline-offset: -2px;
    }
    "#
);

#[derive(Clone, Copy)]
struct RegionFocusState {
    focused: RwSignal<bool, LocalStorage>,
    focus_visible: RwSignal<bool, LocalStorage>,
}

fn attach_region_tracking(node_ref: NodeRef) -> RegionFocusState {
    let focused = RwSignal::new_local(false);
    let focus_visible = RwSignal::new_local(false);

    if let Some(host) = current_host() {
        let host_focus = host.focused();
        let node = node_ref;
        let effect = RenderEffect::new(move |_| {
            let inside = host_focus.get().is_some_and(|active| {
                node.get_untracked() == Some(active) || node.contains(active)
            });
            focused.set(inside);
            if !inside {
                focus_visible.set(false);
            }
        });
        zgui::reactive::on_cleanup_local(move || drop(effect));
    }

    RegionFocusState {
        focused,
        focus_visible,
    }
}

/// Marks a region container with GDS focus-ring styling and active-region tracking attributes
/// (`data-region-focused` and `data-focus-visible`).
#[must_use]
pub fn surface(node_ref: NodeRef) -> Attrs {
    if current_host().is_some() {
        crate::ui::styles::install_ui_stylesheets();
        zgui::view::install_stylesheet("gds-ui-region", SHEET);
    }

    let state = attach_region_tracking(node_ref);
    let focus_out_ref = node_ref;

    Attrs::new()
        .class_toggle(zgui::view::ClassName::new("gds-region-surface"), true)
        .attribute(AttrName::new("data-region-focused"), move || {
            state.focused.get().then(|| "true".to_owned())
        })
        .attribute(AttrName::new("data-focus-visible"), move || {
            state.focus_visible.get().then(|| "true".to_owned())
        })
        .listener(events::FOCUS_IN, ListenerOptions::DEFAULT, move |cx| {
            state.focused.set(true);
            state.focus_visible.set(cx.cause.shows_ring());
        })
        .listener(events::FOCUS_OUT, ListenerOptions::DEFAULT, move |cx| {
            if cx.related.is_none() {
                state.focused.set(false);
                state.focus_visible.set(false);
            } else if let Some(host) = current_host() {
                let inside = host.focused().get_untracked().is_some_and(|active| {
                    focus_out_ref.get_untracked() == Some(active) || focus_out_ref.contains(active)
                });
                if !inside {
                    state.focused.set(false);
                    state.focus_visible.set(false);
                }
            }
        })
}

/// Focusable landmark region applying `.gds-region-surface` styling and focus tracking.
#[component]
pub fn Region(
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    let node_ref = node_ref.unwrap_or_default();
    let region_attrs = surface(node_ref).merged(attrs);

    view! {
        box(
            node_ref = node_ref,
            class = class,
            tabindex = Focus::Sequential,
            a11y:role = Role::Group,
            {..region_attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
