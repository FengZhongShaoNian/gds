//! Reactive mnemonic keytip registry and floating badge overlay.

#[cfg(test)]
mod tests;

use zgui::prelude::*;
use zgui::reactive::{
    LocalStorage, RwSignal, UnsyncCallback, on_cleanup_local, provide_local_context,
    use_local_context,
};
use zgui::{component, view};

/// Registered mnemonic target within the current window.
#[derive(Clone)]
pub struct KeytipBinding {
    pub chord: &'static str,
    pub target: NodeRef,
    pub on_activate: UnsyncCallback<()>,
}

/// Window-local keytip activation state and registry.
#[derive(Clone, Copy)]
pub struct KeytipRegistry {
    active: RwSignal<bool, LocalStorage>,
    entries: RwSignal<Vec<KeytipBinding>, LocalStorage>,
}

impl KeytipRegistry {
    #[must_use]
    pub fn current() -> Option<Self> {
        zgui::reactive::is_ui_thread()
            .then(use_local_context::<Self>)
            .flatten()
    }

    #[must_use]
    pub fn active(self) -> Signal<bool, LocalStorage> {
        self.active.into()
    }

    pub fn set_active(self, active: bool) {
        self.active.set(active);
    }

    pub fn register(self, chord: &'static str, target: NodeRef, on_activate: UnsyncCallback<()>) {
        self.entries.update(|items| {
            items.retain(|item| item.chord != chord);
            items.push(KeytipBinding {
                chord,
                target,
                on_activate,
            });
        });
        let entries = self.entries;
        on_cleanup_local(move || {
            entries.update(|items| items.retain(|item| item.chord != chord));
        });
    }
}

/// Register a mnemonic keytip in the active [`KeytipHost`], if mounted.
pub fn register_keytip(chord: &'static str, target: NodeRef, on_activate: UnsyncCallback<()>) {
    if let Some(registry) = KeytipRegistry::current() {
        registry.register(chord, target, on_activate);
    }
}

/// Provides per-window keytip state and renders mnemonic overlay badges when active.
#[component]
pub fn KeytipHost(
    #[prop(optional)] active: Option<RwSignal<bool, LocalStorage>>,
    children: Children,
) -> impl IntoView {
    let active = active.unwrap_or_else(|| RwSignal::new_local(false));
    let registry = KeytipRegistry {
        active,
        entries: RwSignal::new_local(Vec::new()),
    };
    provide_local_context(registry);
    view! {
        box(class = "gds-keytip-host") {
            {children.into_view_once()}
        }
    }
}

/// Compact mnemonic badge rendered next to keytip-enabled controls.
#[component]
pub fn KeytipBadge(chord: &'static str, #[prop(into, optional)] class: Classes) -> impl IntoView {
    view! {
        text(class = "gds-keytip-badge", class = class) {{chord}}
    }
}
