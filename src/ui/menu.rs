//! Declarative and model-driven popup / context menus wrapping `zgui-ui`.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui::{
    ContextMenuContentProps, ContextMenuProps, ContextMenuTriggerProps, DropdownMenuContentProps,
    DropdownMenuProps, DropdownMenuTriggerProps, MenuCheckboxItemProps, MenuItemProps,
    MenuLabelProps, MenuSeparatorProps, MenuSubContentProps, MenuSubProps, MenuSubTriggerProps,
};
use zgui_ui_primitives::{Binding, Placement};

use crate::ui::Shortcut;
use crate::ui::icon::{IconName, IconProps};

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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: MenuEntry) {
        self.entries.push(entry);
    }
}

/// Compatibility helper wrapping menu state for `bin/files` context popup anchors.
pub fn popup_in(model: MenuModel) -> MenuModel {
    model
}

fn render_entries(entries: Vec<MenuEntry>) -> Vec<AnyView> {
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
                AnyView::new(view! {
                    MenuItem(
                        disabled = disabled,
                        destructive = destructive,
                        shortcut = shortcut_str,
                        on_select = on_select
                    ) {
                        {icon.map(|name| view! { Icon(name = name) }.into_view())}
                        text {{label}}
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
            } => AnyView::new(view! {
                MenuSub {
                    MenuSubTrigger {
                        {icon.map(|name| view! { Icon(name = name) }.into_view())}
                        text {{label}}
                    }
                    MenuSubContent {
                        {render_entries(entries.clone())}
                    }
                }
            }),
            MenuEntry::Label(title) => AnyView::new(view! { MenuLabel {{title}} }),
            MenuEntry::Separator => AnyView::new(view! { MenuSeparator() }),
        })
        .collect()
}

/// Anchored dropdown popup driven by a reactive [`MenuModel`].
#[component]
pub fn PopupMenu(
    #[prop(into, optional)] open: Binding<bool>,
    #[prop(into)] model: Signal<MenuModel, LocalStorage>,
    #[prop(default = Placement::BOTTOM)] placement: Placement,
    trigger: Children,
) -> impl IntoView {
    view! {
        DropdownMenu(open = open) {
            DropdownMenuTrigger {{trigger.into_view_once()}}
            DropdownMenuContent(placement = placement) {
                {render_entries(model.get().entries)}
            }
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
    let build = model;
    view! {
        ContextMenu(open = open) {
            ContextMenuTrigger {{children.into_view_once()}}
            ContextMenuContent {
                {render_entries(build().entries)}
            }
        }
    }
}
