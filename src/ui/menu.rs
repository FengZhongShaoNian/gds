//! Declarative and model-driven popup / context menus wrapping `zgui-ui`.

use zgui::geom::{Css, CssPx, Point};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::view::current_host;
use zgui::vocab::{Key, NamedKey};
use zgui::{component, css, view};
use zgui_ui::{
    ContextMenuContentProps, ContextMenuPlace, ContextMenuProps, ContextMenuTriggerProps,
    DropdownMenuContentProps, DropdownMenuProps, DropdownMenuTriggerProps, MenuCheckboxItemProps,
    MenuItemProps, MenuLabelProps, MenuSeparatorProps, MenuSubContentProps, MenuSubProps,
    MenuSubTriggerProps,
};
use zgui_ui_primitives::{Binding, Placement};

use crate::ui::Shortcut;
use crate::ui::icon::{IconName, IconProps};

/// Minimum width (in CSS px) of a popup or context menu surface.
pub const MIN_WIDTH: f32 = 160.0;
/// Maximum width (in CSS px) of a popup or context menu surface.
pub const MAX_WIDTH: f32 = 340.0;
/// Maximum height (in CSS px) of a scrollable popup or context menu surface.
pub const MAX_HEIGHT: f32 = 440.0;

const SHEET: &str = css!(
    r#"
    .gds-menu-content {
        min-width: 160px;
        max-width: 340px;
        max-height: 440px;
        overflow-y: auto;
    }

    .gds-menu-host {
        display: contents;
    }

    .gds-menu-item-row {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        min-width: 0;
    }

    .gds-menu-item-label {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
    "#
);

/// One entry inside a programmatic [`MenuModel`].
#[derive(Clone)]
pub enum MenuEntry {
    Item {
        label: String,
        icon: Option<IconName>,
        shortcut: Option<Shortcut>,
        disabled: bool,
        destructive: bool,
        on_select: UnsyncCallback<()>,
    },
    Checkbox {
        label: String,
        checked: bool,
        disabled: bool,
        on_toggle: UnsyncCallback<bool>,
    },
    Submenu {
        label: String,
        icon: Option<IconName>,
        entries: Vec<MenuEntry>,
    },
    Label(String),
    Separator,
}

/// Ordered list of menu entries used by dynamic popups and context menus.
#[derive(Clone, Default)]
pub struct MenuModel {
    pub entries: Vec<MenuEntry>,
}

impl MenuModel {
    /// Creates an empty [`MenuModel`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one entry to the menu.
    pub fn push(&mut self, entry: MenuEntry) {
        self.entries.push(entry);
    }

    /// Appends an action item to the menu.
    pub fn item(
        &mut self,
        label: impl Into<String>,
        icon: Option<IconName>,
        shortcut: Option<Shortcut>,
        disabled: bool,
        destructive: bool,
        on_select: UnsyncCallback<()>,
    ) {
        self.push(MenuEntry::Item {
            label: label.into(),
            icon,
            shortcut,
            disabled,
            destructive,
            on_select,
        });
    }

    /// Appends a checkable toggle item to the menu.
    pub fn checkbox(
        &mut self,
        label: impl Into<String>,
        checked: bool,
        disabled: bool,
        on_toggle: UnsyncCallback<bool>,
    ) {
        self.push(MenuEntry::Checkbox {
            label: label.into(),
            checked,
            disabled,
            on_toggle,
        });
    }

    /// Appends a section heading label to the menu.
    pub fn label(&mut self, title: impl Into<String>) {
        self.push(MenuEntry::Label(title.into()));
    }

    /// Appends a visual separator line to the menu.
    pub fn separator(&mut self) {
        self.push(MenuEntry::Separator);
    }
}

/// Compatibility helper wrapping menu state for `bin/files` context popup anchors.
#[must_use]
pub fn popup_in(model: MenuModel) -> MenuModel {
    model
}

/// Opens the enclosing [`ContextMenuHost`]'s menu at window CSS position `position`.
///
/// Returns `true` when invoked inside a [`ContextMenuHost`] subtree.
pub fn popup_at(position: Point<CssPx, Css>) -> bool {
    let Some(place) = ContextMenuPlace::current() else {
        return false;
    };
    place.open_at(position);
    true
}

fn has_leading_column(entries: &[MenuEntry]) -> bool {
    entries.iter().any(|entry| {
        matches!(
            entry,
            MenuEntry::Checkbox { .. }
                | MenuEntry::Item { icon: Some(_), .. }
                | MenuEntry::Submenu { icon: Some(_), .. }
        )
    })
}

fn render_entries(entries: Vec<MenuEntry>) -> Vec<AnyView> {
    let align_column = has_leading_column(&entries);
    entries
        .into_iter()
        .map(|entry| match entry {
            MenuEntry::Item {
                label,
                icon,
                shortcut,
                disabled,
                destructive,
                on_select,
            } => {
                let shortcut_str = shortcut.map(|s| s.label());
                let inset = align_column && icon.is_none();
                AnyView::new(view! {
                    MenuItem(
                        disabled = disabled,
                        destructive = destructive,
                        inset = inset,
                        shortcut = shortcut_str,
                        on_select = on_select
                    ) {
                        row(class = "gds-menu-item-row") {
                            {icon.map(|name| view! { Icon(name = name) }.into_view())}
                            text(class = "gds-menu-item-label") {{label}}
                        }
                    }
                })
            }
            MenuEntry::Checkbox {
                label,
                checked,
                disabled,
                on_toggle,
            } => AnyView::new(view! {
                MenuCheckboxItem(
                    default_checked = checked,
                    disabled = disabled,
                    on_change = on_toggle
                ) {
                    {label}
                }
            }),
            MenuEntry::Submenu {
                label,
                icon,
                entries,
            } => {
                let inset = align_column && icon.is_none();
                AnyView::new(view! {
                    MenuSub {
                        MenuSubTrigger(inset = inset) {
                            row(class = "gds-menu-item-row") {
                                {icon.map(|name| view! { Icon(name = name) }.into_view())}
                                text(class = "gds-menu-item-label") {{label}}
                            }
                        }
                        MenuSubContent(class = "gds-menu-content") {
                            {render_entries(entries.clone())}
                        }
                    }
                })
            }
            MenuEntry::Label(title) => {
                AnyView::new(view! { MenuLabel(inset = align_column) {{title}} })
            }
            MenuEntry::Separator => AnyView::new(view! { MenuSeparator() }),
        })
        .collect()
}

fn install_menu_stylesheet() {
    if current_host().is_some() {
        zgui::view::install_stylesheet("gds-ui-menu", SHEET);
    }
}

/// Anchored dropdown popup driven by a reactive [`MenuModel`].
#[component]
pub fn PopupMenu(
    #[prop(into, optional)] open: Binding<bool>,
    #[prop(into)] model: Signal<MenuModel, LocalStorage>,
    #[prop(default = Placement::BOTTOM)] placement: Placement,
    trigger: Children,
) -> impl IntoView {
    install_menu_stylesheet();
    view! {
        DropdownMenu(open = open) {
            DropdownMenuTrigger {{trigger.into_view_once()}}
            DropdownMenuContent(class = "gds-menu-content", placement = placement) {
                {render_entries(model.get().entries)}
            }
        }
    }
}

fn keyboard_context_anchor(host_ref: &NodeRef) -> Point<CssPx, Css> {
    if let Some(host) = current_host()
        && let Some(focused) = host.focused().get_untracked()
        && let Some(bounds) = host.window_box(focused)
    {
        let scale = host_ref.scale().max(0.01);
        return Point::new(
            CssPx((bounds.origin.x.0 + bounds.size.width.0 * 0.25) / scale),
            CssPx((bounds.origin.y.0 + bounds.size.height.0) / scale),
        );
    }

    if let Some(bounds) = host_ref.window_bounds() {
        let scale = host_ref.scale().max(0.01);
        return Point::new(
            CssPx((bounds.origin.x.0 + bounds.size.width.0 * 0.5) / scale),
            CssPx((bounds.origin.y.0 + bounds.size.height.0 * 0.5) / scale),
        );
    }

    Point::new(CssPx(0.0), CssPx(0.0))
}

#[component]
fn KeyboardContextOpener(children: Children) -> impl IntoView {
    let host_ref = NodeRef::new();
    let place = ContextMenuPlace::current();

    view! {
        box(
            node_ref = host_ref,
            class = "gds-menu-host",
            on:key_down = move |ev: &mut EventCx<'_, zgui::view::events::KeyDown>| {
                if ev.repeat {
                    return;
                }
                let mods = ev.modifiers.held();
                let is_menu_key = ev.key == Key::Named(NamedKey::ContextMenu) && mods.is_empty();
                let is_shift_f10 =
                    ev.key == Key::Named(NamedKey::F10) && mods == zgui::vocab::Modifiers::SHIFT;
                if (is_menu_key || is_shift_f10)
                    && let Some(place) = place
                {
                    let anchor = keyboard_context_anchor(&host_ref);
                    place.open_at(anchor);
                    ev.prevent_default();
                    ev.stop_propagation();
                }
            }
        ) {
            {children.into_view_once()}
        }
    }
}

/// Region wrapper providing a pointer/keyboard-anchored context menu from a builder closure.
#[component]
pub fn ContextMenuHost(
    #[prop(into, optional)] open: Binding<bool>,
    model: std::rc::Rc<dyn Fn() -> MenuModel>,
    children: Children,
) -> impl IntoView {
    install_menu_stylesheet();
    let build = model;
    view! {
        ContextMenu(open = open) {
            ContextMenuTrigger {
                KeyboardContextOpener {
                    {children.into_view_once()}
                }
            }
            ContextMenuContent(class = "gds-menu-content") {
                {render_entries(build().entries)}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui::reactive::{Mounted, UnsyncCallback, install};

    use super::{MenuEntry, MenuModel, has_leading_column, popup_in};
    use crate::ui::icon::IconName;

    #[test]
    fn menu_model_tracks_leading_column_alignment() {
        install().ok();
        let scope = Mounted::new();
        scope.with(|| {
            let mut plain = MenuModel::new();
            plain.item(
                "Rename",
                None,
                None,
                false,
                false,
                UnsyncCallback::new(|()| {}),
            );
            assert!(!has_leading_column(&plain.entries));

            let mut mixed = popup_in(MenuModel::new());
            mixed.item(
                "Copy",
                Some(IconName::Copy),
                None,
                false,
                false,
                UnsyncCallback::new(|()| {}),
            );
            mixed.push(MenuEntry::Separator);
            mixed.item(
                "Paste",
                None,
                None,
                false,
                false,
                UnsyncCallback::new(|()| {}),
            );
            assert!(has_leading_column(&mixed.entries));
        });
        scope.unmount();
    }
}
