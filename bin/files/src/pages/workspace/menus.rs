//! Application and selection menus, and the persisted column-layout editor.

mod pages;

use std::path::PathBuf;

use gds::app::Gds;
use gds::appearance::tokens::DataListStyle;
use gds::i18n;
use gds::ui::button::{ApplicationMenuButtonProps, ButtonSize, IconButtonProps};
use gds::ui::feedback::dialog::{
    DialogActionsProps, DialogContentProps, DialogDecision, DialogFooterProps, DialogHeaderProps,
    DialogProps, DialogTitleProps,
};
use gds::ui::menu::MenuEntry;
use gds::ui::{IconName, IconProps};
use zgui::prelude::*;
use zgui::reactive::{
    RenderEffect, RwSignal, UnsyncCallback, enter_non_reactive_zone, on_cleanup_local,
};
use zgui::task::spawn_local;
use zgui::view;
use zgui::view::events;
use zgui::vocab::{Key, KeyEvent, NamedKey, PointerButton};
use zgui_ui::menu::{
    MenuCheckboxItemProps, MenuContentProps, MenuContext, MenuItemProps, MenuLabelProps,
    MenuSeparatorProps, MenuSubContentProps, MenuSubProps, MenuSubTriggerProps,
};
use zgui_ui::overlay::OverlayState;
use zgui_ui::{
    ButtonProps, ButtonVariant, CheckboxProps, Checked, SelectContentProps, SelectItemProps,
    SelectProps, SelectTriggerProps, SelectValueProps,
};
use zgui_ui_primitives::{Align, Binding, Placement, Side};

use super::geometry::RectF;
use super::model::{Contents, Pane, Prompt, Row, RowKey};
use super::ui::Browser;
use crate::entities::file::ViewMode;
use crate::entities::file::columns::{Column, Layout};
use crate::entities::file::ui::FileIcon;
use crate::entities::location::{Location, scheme::TagAddress, volumes};
use crate::shared::lib::clipboard::Clipboard;
use crate::shared::ui::actions::Command;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct DirectoryTarget {
    tab: u64,
    path: PathBuf,
}

impl DirectoryTarget {
    fn current(browser: &Browser) -> Option<Self> {
        browser.read_untracked(|state| {
            let tab = state.tab();
            tab.navigation
                .path()
                .filter(|_| !tab.navigation.is_loading())
                .map(|path| Self {
                    tab: tab.id,
                    path: path.to_owned(),
                })
        })
    }

    fn still_current(&self, browser: &Browser) -> bool {
        browser.read(|state| {
            state.tab().id == self.tab && state.tab().navigation.path() == Some(self.path.as_path())
        })
    }

    fn still_current_untracked(&self, browser: &Browser) -> bool {
        browser.read_untracked(|state| {
            state.tab().id == self.tab && state.tab().navigation.path() == Some(self.path.as_path())
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct SelectionTarget {
    tab: u64,
    location: Location,
    row: RowKey,
    selected: crate::shared::lib::selection::Selection<RowKey>,
}

impl SelectionTarget {
    fn current(browser: &Browser, row: RowKey) -> Self {
        browser.read_untracked(|state| Self {
            tab: state.tab().id,
            location: state.tab().navigation.location().clone(),
            row,
            selected: state.tab().navigation.view().selection.clone(),
        })
    }

    fn still_current(&self, browser: &Browser) -> bool {
        browser.read(|state| {
            let tab = state.tab();
            tab.id == self.tab
                && *tab.navigation.location() == self.location
                && tab.rows.iter().any(|row| row.key() == self.row)
                && tab.navigation.view().selection == self.selected
        })
    }
}

#[derive(Clone, Copy)]
enum DirectoryAction {
    NewFolder,
    Paste,
    InternalTerminal,
    Terminal,
    Refresh,
}

impl DirectoryAction {
    fn label(self) -> &'static str {
        match self {
            Self::NewFolder => "new-folder",
            Self::Paste => "paste-files",
            Self::InternalTerminal => "files-terminal-open-here",
            Self::Terminal => "terminal-open-external",
            Self::Refresh => "refresh",
        }
    }

    fn available(self, browser: &Browser, target: &DirectoryTarget) -> bool {
        target.still_current_untracked(browser)
            && match self {
                Self::NewFolder | Self::Paste => {
                    !browser.0.volumes.removal_in_progress(&target.path)
                        && !browser.read_untracked(|state| {
                            state.operation.busy() || state.tab().navigation.is_loading()
                        })
                        && (!matches!(self, Self::Paste)
                            || Clipboard::current().can_paste_untracked())
                }
                Self::InternalTerminal => {
                    !browser.0.volumes.removal_in_progress(&target.path)
                        && browser.0.preferences.snapshot().terminal_enabled()
                        && browser.read_untracked(|state| state.picker.is_none())
                }
                Self::Terminal => {
                    !browser.0.volumes.removal_in_progress(&target.path)
                        && browser.0.preferences.snapshot().terminal_enabled()
                }
                Self::Refresh => true,
            }
    }

    fn run(self, browser: &Browser, target: DirectoryTarget) {
        if !target.still_current_untracked(browser) {
            browser.report_notice(i18n::text_untracked("location-changed"));
            return;
        }
        if !self.available(browser, &target) {
            return;
        }
        match self {
            Self::NewFolder => browser.new_folder(),
            Self::Paste => browser.paste(),
            Self::InternalTerminal => browser.new_terminal_at(target.path),
            Self::Terminal => browser.open_external_terminal(target.path),
            Self::Refresh => browser.refresh(),
        }
    }
}

fn command_icon(command: Command) -> Option<IconName> {
    Some(match command {
        Command::NewFolder => IconName::FolderPlus,
        Command::Copy => IconName::Copy,
        Command::Cut => IconName::Scissors,
        Command::Paste => IconName::ClipboardPaste,
        Command::CopyPaths => IconName::ClipboardCopy,
        Command::Open => IconName::ExternalLink,
        Command::OpenInNewTab => IconName::Folder,
        Command::Rename => IconName::TextCursor,
        Command::Trash
        | Command::DeletePermanent
        | Command::PurgeSelection
        | Command::EmptyTrash => IconName::Trash,
        Command::RestoreSelection | Command::Undo => IconName::Undo,
        Command::Redo => IconName::Redo,
        Command::Refresh => IconName::RefreshCw,
        Command::MountVolume => IconName::Plug,
        Command::UnmountVolume => IconName::Unplug,
        Command::Favorite => IconName::Star,
        Command::Settings => IconName::Settings,
        Command::Help => IconName::CircleQuestionMark,
        _ => return None,
    })
}

#[derive(Clone)]
pub(super) enum Surface {
    Closed,
    Application,
    Page,
    Sort,
    Selection {
        target: SelectionTarget,
        x: f32,
        y: f32,
    },
    Directory {
        target: DirectoryTarget,
        x: f32,
        y: f32,
    },
    Trash {
        tab: u64,
        x: f32,
        y: f32,
    },
    Tab {
        id: u64,
        x: f32,
        y: f32,
    },
    Status(super::status::Kind),
    Columns {
        location: Location,
        layout: RwSignal<Layout, LocalStorage>,
        scope: RwSignal<ColumnScope, LocalStorage>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ColumnScope {
    Location,
    Default,
}

impl ColumnScope {
    fn as_value(self) -> &'static str {
        match self {
            Self::Location => "location",
            Self::Default => "default",
        }
    }

    fn from_value(value: &str) -> Self {
        match value {
            "default" => Self::Default,
            _ => Self::Location,
        }
    }
}

pub(super) struct Menus {
    pub(super) surface: RwSignal<Surface, LocalStorage>,
}

impl Menus {
    pub fn new() -> Self {
        Self {
            surface: RwSignal::new_local(Surface::Closed),
        }
    }

    pub fn is_open(&self) -> bool {
        !matches!(self.surface.get_untracked(), Surface::Closed)
    }

    pub(super) fn close(&self) {
        self.surface.set(Surface::Closed);
    }
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

fn render_menu_entries(entries: Vec<MenuEntry>) -> Vec<AnyView> {
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
                        row(class = "gds-files-menu-item-row") {
                            {{icon.map(|name| AnyView::new(view! { Icon(name = name) }))}}
                            text(class = "gds-files-menu-item-label") {{label}}
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
                let entries = StoredValue::new_local(entries);
                AnyView::new(view! {
                    MenuSub {
                        MenuSubTrigger(inset = inset) {
                            row(class = "gds-files-menu-item-row") {
                                {{icon.map(|name| AnyView::new(view! { Icon(name = name) }))}}
                                text(class = "gds-files-menu-item-label") {{label}}
                            }
                        }
                        MenuSubContent(class = "gds-files-menu-content") {
                            {{entries.with_value(|list| render_menu_entries(list.clone()))}}
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

fn anchored_menu_surface(
    anchor: NodeRef,
    open: Signal<bool, LocalStorage>,
    on_close: UnsyncCallback<()>,
    placement: Placement,
    build_entries: std::rc::Rc<dyn Fn() -> Vec<MenuEntry>>,
) -> AnyView {
    let overlay = OverlayState::new(
        Binding::controlled(open, move |next: bool| {
            if !next {
                on_close.run(());
            }
        }),
        false,
        None,
    )
    .anchored_to(anchor);
    MenuContext::new(overlay).provide();
    AnyView::new(view! {
        MenuContent(
            state = overlay,
            placement = placement,
            class = "gds-files-menu-content"
        ) {
            {{render_menu_entries(build_entries())}}
        }
    })
}

impl Browser {
    pub(super) fn menu_key(&self, event: &KeyEvent) -> bool {
        if !self.0.menus.is_open() {
            return false;
        }
        if event.key == Key::Named(NamedKey::Escape) {
            self.0.menus.close();
            return true;
        }
        false
    }

    fn menu_action(
        &self,
        _label: &'static str,
        action: impl Fn(&Browser) + 'static,
    ) -> UnsyncCallback<()> {
        let weak = self.downgrade();
        UnsyncCallback::new(move |()| {
            if let Some(browser) = weak.upgrade() {
                browser.0.owner.with(|| action(&browser));
            }
        })
    }

    fn menu_item_entry(
        &self,
        label_id: &'static str,
        icon: Option<IconName>,
        shortcut: Option<Command>,
        disabled: bool,
        action: impl Fn(&Browser) + 'static,
    ) -> MenuEntry {
        MenuEntry::Item {
            label: i18n::text_untracked(label_id),
            icon,
            shortcut: shortcut.map(Command::shortcut),
            disabled,
            destructive: false,
            on_select: self.menu_action(label_id, action),
        }
    }

    fn command_item(&self, command: Command, action: impl Fn(&Browser) + 'static) -> MenuEntry {
        MenuEntry::Item {
            label: i18n::text_untracked(command.label_id()),
            icon: command_icon(command),
            shortcut: Some(command.shortcut()),
            disabled: false,
            destructive: false,
            on_select: self.menu_action(command.label_id(), action),
        }
    }

    fn paste_item(&self) -> MenuEntry {
        MenuEntry::Item {
            label: i18n::text_untracked(Command::Paste.label_id()),
            icon: Some(IconName::ClipboardPaste),
            shortcut: Some(Command::Paste.shortcut()),
            disabled: !Clipboard::current().can_paste_untracked(),
            destructive: false,
            on_select: self.menu_action(Command::Paste.label_id(), Browser::paste),
        }
    }

    fn app_entries(&self) -> Vec<MenuEntry> {
        let mut entries = vec![
            self.command_item(Command::NewWindow, |browser| {
                let path = browser.read_untracked(|state| {
                    state
                        .tab()
                        .navigation
                        .path()
                        .unwrap_or(&state.home)
                        .to_owned()
                });
                (browser.0.new_window)(path);
            }),
            self.command_item(Command::Settings, Browser::show_settings),
            self.command_item(Command::Help, Browser::show_help),
        ];
        if Gds::current().ui().logs().is_enabled() {
            entries.push(MenuEntry::Item {
                label: i18n::text_untracked("logs-title"),
                icon: None,
                shortcut: None,
                disabled: false,
                destructive: false,
                on_select: UnsyncCallback::new(|()| {
                    gds::ui::logs::open();
                }),
            });
        }
        entries.push(self.menu_item_entry("about", None, None, false, Browser::show_about));
        entries
    }

    pub(super) fn app_menu(&self) -> AnyView {
        let anchor = NodeRef::new();
        let browser = self.clone();
        let entries_browser = self.clone();
        let close_browser = self.clone();
        let activate_browser = self.clone();
        let open = Signal::derive_local({
            let browser = self.clone();
            move || matches!(browser.0.menus.surface.get(), Surface::Application)
        });
        let menu_content = anchored_menu_surface(
            anchor,
            open,
            UnsyncCallback::new(move |()| close_browser.0.menus.close()),
            Placement::new(Side::Bottom, Align::Start),
            std::rc::Rc::new(move || entries_browser.app_entries()),
        );
        AnyView::new(view! {
            box(class = "gds-files-menu-anchor") {
                ApplicationMenuButton(
                    node_ref = anchor,
                    label = Signal::derive_local(|| i18n::text("app-menu")),
                    application = gds::ui::about::Application::try_cargo("gds-files"),
                    shortcut = Some(Command::Menu.shortcut()),
                    keytip = Some("M"),
                    active = open,
                    on_activate = UnsyncCallback::new(move |()| activate_browser.show_app_menu()),
                    on:click = move |_| browser.show_app_menu()
                ) {}
                {menu_content}
            }
        })
    }

    pub(super) fn show_app_menu(&self) {
        self.0.menus.surface.update(|surface| {
            *surface = if matches!(surface, Surface::Application) {
                Surface::Closed
            } else {
                Surface::Application
            };
        });
    }

    pub(super) fn page_menu(&self) -> AnyView {
        let anchor = NodeRef::new();
        let browser = self.clone();
        let entries_browser = self.clone();
        let close_browser = self.clone();
        let activate_browser = self.clone();
        let open = Signal::derive_local({
            let browser = self.clone();
            move || matches!(browser.0.menus.surface.get(), Surface::Page)
        });
        let menu_content = anchored_menu_surface(
            anchor,
            open,
            UnsyncCallback::new(move |()| close_browser.0.menus.close()),
            Placement::new(Side::Bottom, Align::End),
            std::rc::Rc::new(move || entries_browser.page_entries()),
        );
        AnyView::new(view! {
            box(class = "gds-files-menu-anchor") {
                IconButton(
                    node_ref = anchor,
                    icon = IconName::Ellipsis,
                    label = Signal::derive_local(|| i18n::text("page-tools")),
                    keytip = Some("O"),
                    active = open,
                    on_activate = UnsyncCallback::new(move |()| activate_browser.toggle_page_menu()),
                    on:click = move |_| browser.toggle_page_menu()
                ) {}
                {menu_content}
            }
        })
    }

    fn toggle_page_menu(&self) {
        self.0.menus.surface.update(|surface| {
            *surface = if matches!(surface, Surface::Page) {
                Surface::Closed
            } else {
                Surface::Page
            };
        });
    }

    fn page_entries(&self) -> Vec<MenuEntry> {
        if let Some(entries) = self.special_page_entries() {
            return entries;
        }
        let grid = self.read_untracked(|state| state.tab().mode == ViewMode::Grid);
        let favorite = self
            .read_untracked(|state| state.places.is_favorite(state.tab().navigation.location()));
        let columns = self.configurable_columns();
        let show_hidden = self.0.preferences.snapshot().files.show_hidden;
        let compact = self.compact_untracked();

        let mut entries = vec![MenuEntry::Item {
            label: i18n::text_untracked(if grid { "view-list" } else { "view-grid" }),
            icon: Some(if grid {
                IconName::List
            } else {
                IconName::LayoutGrid
            }),
            shortcut: Some(Command::ViewMode.shortcut()),
            disabled: false,
            destructive: false,
            on_select: self.menu_action("view-toggle", Browser::toggle_view),
        }];
        entries.push(MenuEntry::Checkbox {
            label: i18n::text_untracked(if favorite {
                "favorite-remove"
            } else {
                "favorite-add"
            }),
            checked: favorite,
            disabled: false,
            on_toggle: {
                let weak = self.downgrade();
                UnsyncCallback::new(move |_| {
                    if let Some(browser) = weak.upgrade() {
                        browser.0.owner.with(|| browser.favorite());
                    }
                })
            },
        });
        entries.push(self.menu_item_entry(
            "columns-configure",
            Some(IconName::Columns3Cog),
            Some(Command::Columns),
            grid || !columns,
            Browser::configure_columns,
        ));
        entries.push(MenuEntry::Checkbox {
            label: i18n::text_untracked(Command::Hidden.label_id()),
            checked: show_hidden,
            disabled: false,
            on_toggle: {
                let weak = self.downgrade();
                UnsyncCallback::new(move |_| {
                    if let Some(browser) = weak.upgrade() {
                        browser.0.owner.with(|| {
                            let visible = browser.0.preferences.snapshot().files.show_hidden;
                            browser.edit_preferences(|preferences| {
                                preferences.files.show_hidden = !visible;
                            });
                            browser.refresh();
                        });
                    }
                })
            },
        });
        entries.push(self.menu_item_entry(
            Command::Details.label_id(),
            Some(IconName::PanelRight),
            Some(Command::Details),
            compact,
            Browser::toggle_details,
        ));
        entries.push(self.command_item(Command::Refresh, Browser::refresh));
        let sort = self.sort_entries();
        if !sort.is_empty() {
            entries.push(MenuEntry::Submenu {
                label: i18n::text_untracked("sort"),
                icon: Some(IconName::ArrowDownWideNarrow),
                entries: sort,
            });
        }
        entries
    }

    pub(super) fn show_context_menu(&self) {
        let Some(bounds) = self.0.list.window_bounds() else {
            return;
        };
        let scale = self.0.list.scale().max(1.0);
        let port_x = bounds.origin.x.0 / scale;
        let port_y = bounds.origin.y.0 / scale;
        let port_w = (bounds.size.width.0 / scale).max(1.0);
        let port_h = (bounds.size.height.0 / scale).max(1.0);

        let (target, grid) = self.read_untracked(|state| {
            let tab = state.tab();
            let target = tab
                .navigation
                .view()
                .cursor
                .as_ref()
                .filter(|key| tab.navigation.view().selection.contains(key))
                .and_then(|key| tab.rows.iter().position(|row| row.key() == *key))
                .or_else(|| {
                    tab.rows
                        .iter()
                        .position(|row| tab.navigation.view().selection.contains(&row.key()))
                })
                .map(|index| (index, tab.rows[index].key()));
            (target, tab.mode == ViewMode::Grid)
        });

        if let Some((index, target)) = target {
            let tile = if grid {
                self.grid_geometry_untracked().tile(index, scale)
            } else {
                let row = DataListStyle::ROW_HEIGHT;
                RectF::new(0.0, row * index as f32, 0.0, row)
            };
            let scroll = self.0.list.scroll_position().offset;
            let sx = scroll.x.0 / scale;
            let sy = scroll.y.0 / scale;
            let x = port_x + (tile.x - sx + 11.0).clamp(0.0, port_w);
            let y = port_y + ((tile.y + tile.height) - sy).clamp(0.0, port_h);
            self.0.menus.surface.set(Surface::Selection {
                target: SelectionTarget::current(self, target),
                x,
                y,
            });
        } else if self
            .read_untracked(|state| matches!(state.tab().navigation.location(), Location::Trash))
        {
            let tab = self.read_untracked(|state| state.tab().id);
            self.0.menus.surface.set(Surface::Trash {
                tab,
                x: port_x + 11.0,
                y: port_y + 11.0,
            });
        } else if let Some(target) = DirectoryTarget::current(self) {
            self.0.menus.surface.set(Surface::Directory {
                target,
                x: port_x + 11.0,
                y: port_y + 11.0,
            });
        }
    }

    pub(super) fn row_menu(&self, target_view: AnyView, index: usize) -> AnyView {
        let Some(row) = self.read_untracked(|state| state.tab().rows.get(index).cloned()) else {
            return target_view;
        };
        let clicked = row.key();
        let open = self.clone();
        AnyView::new(view! {
            box(
                class = "gds-files-menu-wrap",
                on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                    if ev.button == Some(PointerButton::Secondary) {
                        ev.stop_propagation();
                        let current_index = open.read_untracked(|state| {
                            state.tab().rows.iter().position(|row| row.key() == clicked)
                        });
                        if let Some(current_index) = current_index {
                            open.select_context(current_index);
                            open.0.menus.surface.set(Surface::Selection {
                                target: SelectionTarget::current(&open, clicked.clone()),
                                x: ev.position.x.0,
                                y: ev.position.y.0,
                            });
                        }
                    }
                }
            ) {
                {target_view}
            }
        })
    }

    pub(super) fn sidebar_volume_menu(
        &self,
        target_view: AnyView,
        volume: &volumes::Volume,
    ) -> AnyView {
        let browser = self.clone();
        let volume = volume.clone();
        let model = std::rc::Rc::new(move || {
            let mut menu = gds::ui::menu::MenuModel::new();
            for entry in browser.selection_entries(&Row::Volume(volume.clone())) {
                menu.push(entry);
            }
            menu
        });
        AnyView::new(view! {
            gds::ui::menu::ContextMenuHost(model = model) {{target_view}}
        })
    }

    pub(super) fn directory_menu(&self, target_view: AnyView) -> AnyView {
        let open = self.clone();
        let trigger = move |x: f32, y: f32| {
            if let Some(tab) = open.read_untracked(|state| {
                matches!(state.tab().navigation.location(), Location::Trash)
                    .then_some(state.tab().id)
            }) {
                open.0.menus.surface.set(Surface::Trash { tab, x, y });
            } else if let Some(target) = DirectoryTarget::current(&open) {
                open.0
                    .menus
                    .surface
                    .set(Surface::Directory { target, x, y });
            }
        };
        let trigger_menu = trigger.clone();
        AnyView::new(view! {
            box(
                class = "gds-files-directory-surface",
                on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                    if ev.button == Some(PointerButton::Secondary) {
                        ev.stop_propagation();
                        trigger(ev.position.x.0, ev.position.y.0);
                    }
                },
                on:context_menu = move |ev: &mut EventCx<'_, events::ContextMenu>| {
                    ev.prevent_default();
                    ev.stop_propagation();
                    trigger_menu(ev.position.x.0, ev.position.y.0);
                }
            ) {
                {target_view}
            }
        })
    }

    fn directory_entries(&self, target: DirectoryTarget) -> Vec<MenuEntry> {
        let item = |action: DirectoryAction, icon: IconName, command: Option<Command>| {
            let browser = self.clone();
            let target_run = target.clone();
            let disabled = !action.available(self, &target);
            MenuEntry::Item {
                label: i18n::text_untracked(action.label()),
                icon: Some(icon),
                shortcut: command.map(Command::shortcut),
                disabled,
                destructive: false,
                on_select: UnsyncCallback::new(move |()| {
                    browser
                        .0
                        .owner
                        .with(|| action.run(&browser, target_run.clone()));
                }),
            }
        };

        let mut entries = vec![
            item(
                DirectoryAction::NewFolder,
                IconName::FolderPlus,
                Some(Command::NewFolder),
            ),
            MenuEntry::Separator,
            item(
                DirectoryAction::Paste,
                IconName::ClipboardPaste,
                Some(Command::Paste),
            ),
            MenuEntry::Separator,
        ];
        if self.0.preferences.snapshot().terminal_enabled() {
            if self.read_untracked(|state| state.picker.is_none()) {
                entries.push(item(
                    DirectoryAction::InternalTerminal,
                    IconName::Terminal,
                    None,
                ));
            }
            entries.push(item(DirectoryAction::Terminal, IconName::Terminal, None));
        }
        entries.push(item(
            DirectoryAction::Refresh,
            IconName::RefreshCw,
            Some(Command::Refresh),
        ));
        entries
    }

    pub(super) fn tab_menu(&self, target_view: AnyView, id: u64) -> AnyView {
        let open = self.clone();
        AnyView::new(view! {
            box(
                class = "gds-files-menu-wrap",
                on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                    if ev.button == Some(PointerButton::Secondary) {
                        ev.stop_propagation();
                        open.0.menus.surface.set(Surface::Tab {
                            id,
                            x: ev.position.x.0,
                            y: ev.position.y.0,
                        });
                    }
                }
            ) {
                {target_view}
            }
        })
    }

    fn tab_entries(&self, id: u64, pinned: bool) -> Vec<MenuEntry> {
        let pin = self.clone();
        let close = self.clone();
        let add = self.clone();
        vec![
            MenuEntry::Item {
                label: i18n::text_untracked(if pinned { "unpin-tab" } else { "pin-tab" }),
                icon: None,
                shortcut: None,
                disabled: false,
                destructive: false,
                on_select: UnsyncCallback::new(move |()| {
                    if let Some(index) =
                        pin.read_untracked(|state| state.tabs.iter().position(|tab| tab.id == id))
                    {
                        pin.0.owner.with(|| pin.toggle_pin(index));
                    }
                }),
            },
            MenuEntry::Item {
                label: i18n::text_untracked(Command::NewTab.label_id()),
                icon: None,
                shortcut: Some(Command::NewTab.shortcut()),
                disabled: false,
                destructive: false,
                on_select: UnsyncCallback::new(move |()| add.0.owner.with(|| add.new_tab(None))),
            },
            MenuEntry::Item {
                label: i18n::text_untracked(Command::CloseTab.label_id()),
                icon: None,
                shortcut: Some(Command::CloseTab.shortcut()),
                disabled: false,
                destructive: false,
                on_select: UnsyncCallback::new(move |()| {
                    if let Some(index) =
                        close.read_untracked(|state| state.tabs.iter().position(|tab| tab.id == id))
                    {
                        close.0.owner.with(|| close.close_tab(index));
                    }
                }),
            },
        ]
    }

    fn selection_entries(&self, row: &Row) -> Vec<MenuEntry> {
        match row {
            Row::Trash(_) => self.trash_entries(true),
            Row::Volume(volume) => {
                let busy = self.0.volumes.is_busy_untracked(&volume.id)
                    || self.read_untracked(|state| state.operation.busy());
                let rename = volume.clone();
                let mut items = Vec::new();
                if !volume.mount_points.is_empty() {
                    let open = volume.id.clone();
                    let new_tab = open.clone();
                    items.push(MenuEntry::Item {
                        label: i18n::text_untracked("open"),
                        icon: Some(IconName::Folder),
                        shortcut: Some(Command::Open.shortcut()),
                        disabled: busy,
                        destructive: false,
                        on_select: self.menu_action("open", move |browser| {
                            browser.open_volume(&open, false);
                        }),
                    });
                    items.push(MenuEntry::Item {
                        label: i18n::text_untracked(Command::OpenInNewTab.label_id()),
                        icon: Some(IconName::Folder),
                        shortcut: Some(Command::OpenInNewTab.shortcut()),
                        disabled: busy,
                        destructive: false,
                        on_select: self.menu_action(
                            Command::OpenInNewTab.label_id(),
                            move |browser| {
                                browser.open_volume(&new_tab, true);
                            },
                        ),
                    });
                }
                if volume.can_set_label() {
                    items.push(MenuEntry::Item {
                        label: i18n::text_untracked("rename"),
                        icon: Some(IconName::TextCursor),
                        shortcut: Some(Command::Rename.shortcut()),
                        disabled: busy,
                        destructive: false,
                        on_select: self.menu_action("rename", move |browser| {
                            browser.rename_volume(rename.clone());
                        }),
                    });
                }
                if volume.mount_points.is_empty() {
                    let id = volume.id.clone();
                    let enabled = volume.mountable && !volume.locked && !busy;
                    items.push(MenuEntry::Item {
                        label: i18n::text_untracked("device-mount"),
                        icon: Some(IconName::Plug),
                        shortcut: Some(Command::MountVolume.shortcut()),
                        disabled: !enabled,
                        destructive: false,
                        on_select: self.menu_action("device-mount", move |browser| {
                            browser.change_volume(volumes::DeviceAction::Mount(id.clone()));
                        }),
                    });
                } else if volume.can_unmount() {
                    let id = volume.id.clone();
                    items.push(MenuEntry::Item {
                        label: i18n::text_untracked("device-unmount"),
                        icon: Some(IconName::Unplug),
                        shortcut: Some(Command::UnmountVolume.shortcut()),
                        disabled: busy,
                        destructive: false,
                        on_select: self.menu_action("device-unmount", move |browser| {
                            browser.change_volume(volumes::DeviceAction::Unmount(id.clone()));
                        }),
                    });
                }
                let snapshot = self.0.volumes.snapshot_untracked();
                if let Some(drive) = volume.drive.as_ref().and_then(|id| snapshot.drive(id)) {
                    if drive.can_eject {
                        let id = volume.id.clone();
                        items.push(MenuEntry::Item {
                            label: i18n::text_untracked("device-eject"),
                            icon: Some(IconName::Unplug),
                            shortcut: None,
                            disabled: busy,
                            destructive: false,
                            on_select: self.menu_action("device-eject", move |browser| {
                                browser.change_volume(volumes::DeviceAction::Eject(id.clone()));
                            }),
                        });
                    }
                    if drive.can_power_off {
                        let id = volume.id.clone();
                        items.push(MenuEntry::Item {
                            label: i18n::text_untracked("device-power-off"),
                            icon: None,
                            shortcut: None,
                            disabled: false,
                            destructive: false,
                            on_select: self.menu_action("device-power-off", move |browser| {
                                browser.change_volume(volumes::DeviceAction::PowerOff(id.clone()));
                            }),
                        });
                    }
                }
                items
            }
            Row::Tag(_) => self.place_entries(row),
            Row::Remote(_) => vec![
                self.command_item(Command::Open, Browser::open_selected),
                self.command_item(Command::OpenInNewTab, Browser::open_selected_tab),
                self.command_item(Command::Copy, |browser| browser.copy(false)),
                self.paste_item(),
                self.menu_item_entry(
                    "remote-download",
                    Some(IconName::Download),
                    None,
                    false,
                    Browser::download_selected_remote,
                ),
                self.command_item(Command::Rename, Browser::rename),
                self.command_item(Command::DeletePermanent, |browser| browser.delete(true)),
                MenuEntry::Separator,
                self.command_item(Command::Favorite, Browser::favorite),
                self.menu_item_entry(
                    "remote-disconnect",
                    Some(IconName::Unplug),
                    None,
                    false,
                    Browser::disconnect_remote,
                ),
            ],
            Row::Place(_) => self.place_entries(row),
            Row::File(_)
                if self.read_untracked(|state| {
                    matches!(state.tab().navigation.location(), Location::Tags(_))
                }) =>
            {
                self.place_entries(row)
            }
            Row::File(entry) => self.file_entries(entry),
        }
    }

    fn trash_entries(&self, selection: bool) -> Vec<MenuEntry> {
        let (count, busy, has_rows) = self.read_untracked(|state| {
            let tab = state.tab();
            (
                tab.navigation.view().selection.len(),
                state.operation.busy() || tab.navigation.is_loading(),
                matches!(&tab.contents, Contents::Trash(entries) if !entries.is_empty()),
            )
        });
        let mut entries = Vec::new();
        if selection {
            let restore = if count == 1 {
                i18n::text_untracked("recycle-restore")
            } else {
                i18n::count_untracked("recycle-restore-selected", count)
            };
            for (label, icon, command, purge) in [
                (restore, IconName::Undo, Command::RestoreSelection, false),
                (
                    i18n::count_untracked("recycle-purge-selected", count),
                    IconName::Trash,
                    Command::PurgeSelection,
                    true,
                ),
            ] {
                let weak = self.downgrade();
                entries.push(MenuEntry::Item {
                    label,
                    icon: Some(icon),
                    shortcut: Some(command.shortcut()),
                    disabled: busy || count == 0,
                    destructive: purge,
                    on_select: UnsyncCallback::new(move |()| {
                        if let Some(browser) = weak.upgrade() {
                            browser.restore_trash(purge, false);
                        }
                    }),
                });
            }
            entries.push(MenuEntry::Separator);
        }
        entries.extend([
            self.command_item(Command::Refresh, Browser::refresh),
            MenuEntry::Separator,
            self.menu_item_entry(
                "recycle-empty-action",
                Some(IconName::Trash),
                Some(Command::EmptyTrash),
                busy || !has_rows,
                |browser| browser.restore_trash(true, true),
            ),
        ]);
        entries
    }

    pub(super) fn place_location(row: &Row) -> Option<Location> {
        match row {
            Row::Place(location) => Some(location.clone()),
            Row::File(entry) => Some(Location::from(entry.path.as_path())),
            Row::Tag(tag) => TagAddress::new(tag).ok().map(Location::Tags),
            _ => None,
        }
    }

    fn place_entries(&self, row: &Row) -> Vec<MenuEntry> {
        let Some(location) = Self::place_location(row) else {
            return Vec::new();
        };
        let local = location.local_path();
        let favorite = self.read_untracked(|state| state.places.is_favorite(&location));
        let recent = self
            .read_untracked(|state| matches!(state.tab().navigation.location(), Location::Recent));
        let target = location.clone();
        let new_tab = location.clone();
        let mut entries = vec![
            self.command_item(Command::Open, move |browser| browser.open(target.clone())),
            self.command_item(Command::OpenInNewTab, move |browser| {
                browser.new_tab(Some(new_tab.clone()));
            }),
        ];
        if local.is_some() {
            entries.push(self.menu_item_entry(
                "tags-show-parent",
                None,
                None,
                false,
                Browser::show_selected_parent,
            ));
        }
        entries.push(MenuEntry::Separator);
        if local.is_some() && !recent {
            entries.push(self.command_item(Command::Copy, |browser| browser.copy(false)));
        }
        entries.push(self.command_item(Command::CopyPaths, Browser::copy_selected_paths));
        entries.push(MenuEntry::Separator);
        let saved = location.clone();
        entries.push(self.menu_item_entry(
            if favorite {
                "place-favorite-remove"
            } else {
                "place-favorite-add"
            },
            Some(IconName::Star),
            Some(Command::Favorite),
            false,
            move |browser| {
                browser.update(|state| state.places.toggle_favorite(saved.clone()));
                browser.save_places();
                browser.refresh();
            },
        ));
        if local.is_some() && self.0.preferences.snapshot().tags.enabled {
            entries.push(self.menu_item_entry(
                "tags-edit",
                Some(IconName::Tag),
                None,
                false,
                Browser::edit_selected_tags,
            ));
        }
        if matches!(row, Row::Tag(_)) {
            let tag = row.clone();
            entries.push(self.menu_item_entry(
                "tags-delete",
                Some(IconName::Trash),
                None,
                false,
                move |browser| browser.delete_tags(vec![tag.clone()]),
            ));
        }
        if !recent {
            entries.push(MenuEntry::Separator);
            entries.push(self.command_item(Command::Refresh, Browser::refresh));
        }
        entries
    }

    fn file_entries(&self, entry: &crate::entities::file::directory::Entry) -> Vec<MenuEntry> {
        let (selected, picker, busy, current_dir) = self.read_untracked(|state| {
            let tab = state.tab();
            (
                tab.selected(),
                state.picker.is_some(),
                state.operation.busy() || tab.navigation.is_loading(),
                tab.navigation.path().map(ToOwned::to_owned),
            )
        });
        let many = selected.len() > 1;
        let count = selected.len();
        let unavailable = selected
            .iter()
            .filter_map(Row::path)
            .any(|path| self.0.volumes.removal_in_progress(path));
        let directories = selected
            .iter()
            .all(|row| matches!(row, Row::File(file) if FileIcon::is_directory(file)));

        let action = |command: Command, plural: &'static str, run: fn(&Browser), disabled: bool| {
            let label = if many {
                i18n::count_untracked(plural, count)
            } else if command == Command::CopyPaths {
                i18n::text_untracked("copy-path")
            } else {
                i18n::text_untracked(command.label_id())
            };
            MenuEntry::Item {
                label,
                icon: command_icon(command),
                shortcut: Some(command.shortcut()),
                disabled: disabled || (command != Command::CopyPaths && unavailable),
                destructive: false,
                on_select: self.menu_action(command.label_id(), run),
            }
        };

        let mut entries = Vec::new();
        if !many {
            entries.push(action(Command::Open, "open", Browser::open_selected, false));
            if !directories && !picker {
                entries.push(self.menu_item_entry(
                    "open-with",
                    Some(IconName::ExternalLink),
                    None,
                    unavailable,
                    Browser::open_selected_with,
                ));
            }
        }
        if directories {
            entries.push(action(
                Command::OpenInNewTab,
                "open-directories-new-tabs",
                Browser::open_selected_tab,
                false,
            ));
        }
        if !entries.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        entries.push(action(
            Command::Copy,
            "copy-selected-files",
            |browser| browser.copy(false),
            busy,
        ));
        entries.push(action(
            Command::Cut,
            "cut-selected-files",
            |browser| browser.copy(true),
            busy,
        ));
        if !many {
            let paste_disabled = busy
                || current_dir.is_none_or(|path| self.0.volumes.removal_in_progress(&path))
                || !Clipboard::current().can_paste_untracked();
            entries.push(self.menu_item_entry(
                "paste-files",
                Some(IconName::ClipboardPaste),
                Some(Command::Paste),
                paste_disabled,
                Browser::paste,
            ));
        }
        entries.push(MenuEntry::Separator);
        entries.push(action(
            Command::CopyPaths,
            "copy-selected-paths",
            Browser::copy_selected_paths,
            selected
                .iter()
                .any(|row| row.path().is_some_and(|p| p.to_str().is_none())),
        ));
        entries.push(MenuEntry::Separator);
        if !many {
            entries.push(action(
                Command::Rename,
                "rename",
                Browser::rename,
                busy || !cfg!(target_os = "linux") || entry.path.file_name().is_none(),
            ));
            if self.0.preferences.snapshot().tags.enabled {
                entries.push(self.menu_item_entry(
                    "tags-edit",
                    Some(IconName::Tag),
                    None,
                    busy || unavailable,
                    Browser::edit_selected_tags,
                ));
            }
            #[cfg(unix)]
            {
                let entry_perm = entry.clone();
                entries.push(self.menu_item_entry(
                    "permissions-title",
                    Some(IconName::Lock),
                    None,
                    busy || unavailable,
                    move |browser| browser.edit_permissions(entry_perm.clone()),
                ));
            }
            entries.push(MenuEntry::Separator);
        }
        entries.push(action(
            Command::Trash,
            "trash-selected",
            |browser| browser.delete(false),
            busy,
        ));
        entries.push(action(
            Command::DeletePermanent,
            "delete-selected",
            |browser| browser.delete(true),
            busy,
        ));
        if !many && self.0.preferences.snapshot().terminal_enabled() {
            let target = if directories {
                Some(entry.path.as_path())
            } else {
                entry.path.parent()
            };
            if let Some(target) = target {
                entries.push(MenuEntry::Separator);
                if !picker {
                    let path = target.to_owned();
                    entries.push(self.menu_item_entry(
                        "files-terminal-open-here",
                        Some(IconName::Terminal),
                        None,
                        unavailable,
                        move |browser| browser.new_terminal_at(path.clone()),
                    ));
                }
                let path = target.to_owned();
                entries.push(self.menu_item_entry(
                    "terminal-open-external",
                    Some(IconName::Terminal),
                    None,
                    unavailable,
                    move |browser| browser.open_external_terminal(path.clone()),
                ));
            }
        }
        entries
    }

    fn open_external_terminal(&self, path: PathBuf) {
        self.blocking(
            "Open external terminal",
            move || {
                gds::service::desktop::Desktop::default()
                    .open_terminal(&path)
                    .map_err(|error| error.to_string())
            },
            |browser, result| {
                if let Err(error) = result {
                    browser.report_notice(error);
                }
            },
        );
    }

    pub(super) fn open_selected_tab(&self) {
        for row in self.read_untracked(|state| state.tab().selected()) {
            let location = match row {
                Row::File(entry) if FileIcon::is_directory(&entry) => {
                    Location::from(entry.path.as_path())
                }
                Row::Place(location) => location,
                Row::Remote(entry) if entry.kind == gds::service::remote::EntryKind::Directory => {
                    Location::Remote(entry.address)
                }
                Row::Volume(volume) if !volume.mount_points.is_empty() => {
                    self.open_volume(&volume.id, true);
                    continue;
                }
                _ => continue,
            };
            self.new_tab(Some(location));
        }
    }

    fn open_selected_with(&self) {
        let paths = self.read_untracked(|state| {
            state
                .tab()
                .selected()
                .iter()
                .filter_map(|row| row.path().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        });
        for path in paths {
            self.open_file(path, gds::service::desktop::OpenMode::ChooseApplication);
        }
    }

    fn show_selected_parent(&self) {
        let paths = self.read_untracked(|state| {
            state
                .tab()
                .selected()
                .iter()
                .filter_map(|row| {
                    match row {
                        Row::Place(location) => location.local_path(),
                        _ => row.path(),
                    }
                    .map(ToOwned::to_owned)
                })
                .collect::<Vec<_>>()
        });
        self.apply_desktop_command(gds::service::desktop::file_manager::Command {
            request: gds::service::desktop::file_manager::Request::ShowItems { paths },
            startup_id: String::new(),
        });
    }

    fn edit_selected_tags(&self) {
        let paths = self.read_untracked(|state| {
            state
                .tab()
                .selected()
                .iter()
                .filter_map(|row| {
                    match row {
                        Row::Place(location) => location.local_path(),
                        _ => row.path(),
                    }
                    .map(ToOwned::to_owned)
                })
                .collect::<Vec<_>>()
        });
        for path in paths {
            self.edit_tags(path);
        }
    }

    pub(super) fn copy_selected_paths(&self) {
        use crate::features::file_operations::{Batch, Error};

        let (rows, collection) = self.read_untracked(|state| {
            let tab = state.tab();
            (
                tab.selected(),
                matches!(
                    tab.navigation.location(),
                    Location::Favorites | Location::Recent | Location::Tags(_)
                ),
            )
        });
        let write_clip = |text: String| {
            if let Some(clipboard) = try_use_clipboard() {
                clipboard.set_text(ClipboardKind::Standard, text);
            }
        };
        if collection {
            let result = rows
                .iter()
                .filter_map(Self::place_location)
                .map(|location| location.uri())
                .collect::<Result<Vec<_>, _>>();
            match result {
                Ok(uris) if !uris.is_empty() => write_clip(uris.join("\n")),
                Ok(_) => {}
                Err(error) => self.report_notice(error.to_string()),
            }
            return;
        }
        let paths = rows
            .iter()
            .filter_map(|row| row.path().map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        let text = if paths.is_empty() {
            rows.into_iter()
                .filter_map(|row| match row {
                    Row::Remote(entry) => Some(Ok(entry.address.uri().to_owned())),
                    Row::Tag(_) => {
                        Self::place_location(&row).map(|location| Ok(location.display_address()))
                    }
                    Row::Place(location) => Some(match location.local_path() {
                        Some(path) => path.to_str().map(ToOwned::to_owned).ok_or_else(|| {
                            Error::NonUtf8Clipboard {
                                path: path.to_owned(),
                            }
                        }),
                        None => Ok(location.display_address()),
                    }),
                    _ => None,
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|addresses| addresses.join("\n"))
        } else {
            Batch::new(paths).and_then(|batch| batch.clipboard_text())
        };
        match text {
            Ok(text) if !text.is_empty() => write_clip(text),
            Ok(_) => {}
            Err(error) => self.report_notice(error.to_string()),
        }
    }

    pub(super) fn change_volume(&self, action: volumes::DeviceAction) {
        if self.read_untracked(|state| state.operation.busy()) {
            self.report_notice(i18n::text_untracked("remote-task-running"));
            return;
        }
        let weak = self.downgrade();
        match self.0.volumes.operate(action) {
            Ok(task) => {
                self.0.owner.with(|| {
                    drop(spawn_local(async move {
                        let result = task.await;
                        if let Some(browser) = weak.upgrade() {
                            if let Err(error) = result {
                                browser.report_notice(error.to_string());
                            }
                            browser.refresh();
                        }
                    }));
                });
            }
            Err(error) => self.report_notice(error.to_string()),
        }
    }

    pub(super) fn show_help(&self) {
        self.open_page(Pane::Help);
    }

    pub(super) fn show_about(&self) {
        self.0.prompt.set(Prompt::About);
    }

    pub(super) fn column_menu(&self) -> AnyView {
        let browser = self.clone();
        AnyView::new(view! {
            row(class = "gds-files-column-controls") {
                {move || browser.column_controls()}
            }
        })
    }

    fn column_controls(&self) -> AnyView {
        let anchor = NodeRef::new();
        let columns_browser = self.clone();
        let columns_act = self.clone();
        let sort_click = self.clone();
        let sort_act = self.clone();
        let entries_browser = self.clone();
        let close_browser = self.clone();
        let configurable = self.configurable_columns();
        let grid = self.read_untracked(|state| state.tab().mode == ViewMode::Grid);
        let sortable = !self.sortable_columns().is_empty();
        let open = Signal::derive_local({
            let browser = self.clone();
            move || matches!(browser.0.menus.surface.get(), Surface::Sort)
        });
        let sort_menu = anchored_menu_surface(
            anchor,
            open,
            UnsyncCallback::new(move |()| close_browser.0.menus.close()),
            Placement::new(Side::Bottom, Align::End),
            std::rc::Rc::new(move || entries_browser.sort_entries()),
        );

        AnyView::new(view! {
            row(class = "gds-files-column-controls") {
                {{sortable.then(|| AnyView::new(view! {
                    box(class = "gds-files-menu-anchor") {
                        IconButton(
                            node_ref = anchor,
                            icon = IconName::ArrowDownWideNarrow,
                            label = Signal::derive_local(|| i18n::text("sort")),
                            shortcut = Some(Command::Sort.shortcut()),
                            keytip = Some("S"),
                            active = open,
                            on_activate = UnsyncCallback::new(move |()| sort_act.show_sort_menu()),
                            on:click = move |_| sort_click.show_sort_menu()
                        ) {}
                        {sort_menu}
                    }
                }))}}
                {{configurable.then(|| AnyView::new(view! {
                    IconButton(
                        icon = IconName::Columns3Cog,
                        label = Signal::derive_local(|| i18n::text("columns-configure")),
                        disabled = Signal::stored_local(grid),
                        shortcut = Some(Command::Columns.shortcut()),
                        keytip = Some("C"),
                        on_activate = UnsyncCallback::new(move |()| columns_act.configure_columns()),
                        on:click = move |_| columns_browser.configure_columns()
                    )
                }))}}
            }
        })
    }

    fn sort_entries(&self) -> Vec<MenuEntry> {
        let current_sort = self.read_untracked(|state| state.tab().sort.key);
        self.sortable_columns()
            .into_iter()
            .map(|column| {
                let weak = self.downgrade();
                MenuEntry::Checkbox {
                    label: i18n::text_untracked(column.label_id()),
                    checked: current_sort == column,
                    disabled: false,
                    on_toggle: UnsyncCallback::new(move |_| {
                        if let Some(browser) = weak.upgrade() {
                            browser.0.owner.with(|| browser.sort_by(column));
                        }
                    }),
                }
            })
            .collect()
    }

    pub(super) fn show_sort_menu(&self) {
        if self.sortable_columns().is_empty() {
            return;
        }
        self.0.menus.surface.update(|surface| {
            *surface = if matches!(surface, Surface::Sort) {
                Surface::Closed
            } else {
                Surface::Sort
            };
        });
    }

    pub(super) fn configure_columns(&self) {
        if !self.configurable_columns()
            || self.read_untracked(|state| state.tab().mode == ViewMode::Grid)
        {
            return;
        }
        let (location, layout) = self.read_untracked(|state| {
            (
                state.tab().navigation.location().clone(),
                state.tab().columns.clone(),
            )
        });
        self.0.menus.surface.set(Surface::Columns {
            location,
            layout: RwSignal::new_local(layout),
            scope: RwSignal::new_local(ColumnScope::Location),
        });
    }

    fn configurable_columns(&self) -> bool {
        self.read_untracked(|state| {
            matches!(
                state.tab().contents,
                Contents::Directory | Contents::Tags { .. }
            ) && !state.tab().is_pure_tag_catalogue()
        })
    }

    pub(super) fn sort_by(&self, column: Column) {
        let id = self.update(|state| {
            let tab = state.tab_mut();
            tab.sort = tab.sort.next(column);
            let id = tab.id;
            let location = tab.navigation.location().clone();
            let sort = tab.sort;
            state.places.set_sort(location, sort);
            id
        });
        self.reproject(id);
        self.save_places();
        self.enrich_directory(id);
    }

    pub(super) fn menu_overlay(&self) -> AnyView {
        let origin = self.clone();
        let validity_fx = RenderEffect::new(move |_| {
            let valid = match origin.0.menus.surface.get() {
                Surface::Selection { target, .. } => target.still_current(&origin),
                Surface::Directory { target, .. } => target.still_current(&origin),
                Surface::Trash { tab, .. } => origin.read(|state| {
                    state.tab().id == tab
                        && matches!(state.tab().navigation.location(), Location::Trash)
                }),
                Surface::Tab { id, .. } => {
                    origin.read(|state| state.tabs.iter().any(|tab| tab.id == id))
                }
                _ => true,
            };
            if !valid {
                origin.0.menus.close();
            }
        });
        on_cleanup_local(move || drop(validity_fx));

        let columns_open = Signal::derive_local({
            let browser = self.clone();
            move || matches!(browser.0.menus.surface.get(), Surface::Columns { .. })
        });
        let close_columns = self.clone();
        let content_columns = self.clone();
        let popup_browser = self.clone();
        let permissions_view = self.permissions_dialog();
        let tags_view = self.tags_dialog();

        AnyView::new(view! {
            box(class = "gds-files-menu-overlay") {
                {move || popup_browser.context_popup()}
                Dialog(
                    open = Binding::controlled(
                        columns_open,
                        move |next: bool| {
                            if !next {
                                close_columns.0.menus.close();
                            }
                        }
                    )
                ) {
                    DialogContent(
                        class = "gds-files-columns-dialog",
                        dismiss_on_outside_press = true,
                        dismiss_on_escape = true
                    ) {
                        {{
                            let c = content_columns.clone();
                            move || c.columns_dialog()
                        }}
                    }
                }
                {permissions_view}
                {tags_view}
            }
        })
    }

    fn context_popup(&self) -> AnyView {
        let surface = self.0.menus.surface.get();
        let Some((x, y, entries)) = ({
            let _z = enter_non_reactive_zone();
            match surface {
                Surface::Selection { target, x, y } => self
                    .read_untracked(|state| {
                        state
                            .tab()
                            .rows
                            .iter()
                            .find(|row| row.key() == target.row)
                            .cloned()
                    })
                    .map(|row| (x, y, self.selection_entries(&row))),
                Surface::Directory { target, x, y } => Some((x, y, self.directory_entries(target))),
                Surface::Trash { x, y, .. } => Some((x, y, self.trash_entries(false))),
                Surface::Tab { id, x, y } => self
                    .read_untracked(|state| {
                        state
                            .tabs
                            .iter()
                            .find(|tab| tab.id == id)
                            .map(|tab| tab.pinned)
                    })
                    .map(|pinned| (x, y, self.tab_entries(id, pinned))),
                _ => None,
            }
        }) else {
            return AnyView::new(view! { box(class = "gds-files-hidden") });
        };

        let anchor = NodeRef::new();
        let close = self.clone();
        let overlay = OverlayState::new(
            Binding::controlled(Signal::stored_local(true), move |next: bool| {
                if !next {
                    close.0.menus.close();
                }
            }),
            true,
            None,
        )
        .anchored_to(anchor);
        MenuContext::new(overlay).provide();

        AnyView::new(view! {
            box(class = "gds-files-context-portal-anchor") {
                box(
                    node_ref = anchor,
                    class = "gds-files-context-point",
                    style:left = format!("{x:.1}px"),
                    style:top = format!("{y:.1}px"),
                    a11y:hidden = true
                )
                MenuContent(
                    state = overlay,
                    placement = Placement::new(Side::Bottom, Align::Start),
                    offset = 0.0,
                    class = "gds-files-menu-content"
                ) {
                    {render_menu_entries(entries.clone())}
                }
            }
        })
    }

    fn columns_dialog(&self) -> AnyView {
        let Surface::Columns {
            location,
            layout,
            scope,
        } = self.0.menus.surface.get()
        else {
            return AnyView::new(view! { box() });
        };

        let browser = self.clone();
        let defaults = self.clone();
        let restore = location.clone();
        let apply = location.clone();
        let confirm = self.clone();

        let scope_value = Signal::derive_local(move || scope.get().as_value().to_owned());
        let address = location.display_address();

        AnyView::new(view! {
            column(class = "gds-files-columns-body") {
                DialogHeader {
                    DialogTitle {{i18n::text("columns-title")}}
                }
                text(class = "gds-files-columns-address") {{address}}
                Select(
                    value = Binding::controlled(
                        scope_value,
                        move |next: String| {
                            scope.set(ColumnScope::from_value(&next));
                        }
                    )
                ) {
                    SelectTrigger {
                        SelectValue(placeholder = i18n::text("columns-scope"))
                    }
                    SelectContent {
                        SelectItem(value = "location") {{i18n::text("columns-scope-folder")}}
                        SelectItem(value = "default") {{i18n::text("columns-scope-global")}}
                    }
                }
                column(class = "gds-files-columns-list") {
                    {{move || {
                        let current = layout.get();
                        let len = current.columns().len();
                        let visible = current.columns().iter().copied().enumerate().map(|(index, column)| {
                            let is_checked = Signal::derive_local(move || {
                                if layout.with(|l| l.contains(column)) {
                                    Checked::Yes
                                } else {
                                    Checked::No
                                }
                            });
                            let required = column.is_required();
                            AnyView::new(view! {
                                row(class = "gds-files-column-option") {
                                    Checkbox(
                                        checked = Binding::controlled(
                                            is_checked,
                                            move |next: Checked| {
                                                layout.update(|l| {
                                                    l.set_visible(column, next == Checked::Yes);
                                                });
                                            }
                                        ),
                                        disabled = Signal::stored_local(required),
                                        class = "gds-files-column-check"
                                    )
                                    text(class = "gds-files-column-name") {{i18n::text(column.label_id())}}
                                    IconButton(
                                        icon = IconName::ChevronUp,
                                        label = i18n::text("columns-move-up"),
                                        size = ButtonSize::IconXs,
                                        disabled = Signal::stored_local(index == 0),
                                        on:click = move |_| {
                                            layout.update(|l| {
                                                l.move_to(column, index.saturating_sub(1));
                                            });
                                        }
                                    )
                                    IconButton(
                                        icon = IconName::ChevronDown,
                                        label = i18n::text("columns-move-down"),
                                        size = ButtonSize::IconXs,
                                        disabled = Signal::stored_local(index + 1 >= len),
                                        on:click = move |_| {
                                            layout.update(|l| {
                                                l.move_to(column, index + 1);
                                            });
                                        }
                                    )
                                }
                            })
                        }).collect::<Vec<_>>();

                        let hidden = Column::ALL
                            .into_iter()
                            .filter(|column| !current.contains(*column))
                            .map(|column| {
                                let is_checked = Signal::derive_local(move || {
                                    if layout.with(|l| l.contains(column)) {
                                        Checked::Yes
                                    } else {
                                        Checked::No
                                    }
                                });
                                AnyView::new(view! {
                                    row(class = "gds-files-column-option") {
                                        Checkbox(
                                            checked = Binding::controlled(
                                                is_checked,
                                                move |next: Checked| {
                                                    layout.update(|l| {
                                                        l.set_visible(column, next == Checked::Yes);
                                                    });
                                                }
                                            ),
                                            class = "gds-files-column-check"
                                        )
                                        text(class = "gds-files-column-name") {{i18n::text(column.label_id())}}
                                    }
                                })
                            })
                            .collect::<Vec<_>>();

                        view! {
                            column(class = "gds-files-columns-list") {
                                {visible}
                                {hidden}
                            }
                        }
                    }}}
                }
                Button(
                    variant = ButtonVariant::Outline,
                    on:click = move |_| {
                        defaults.apply_columns(restore.clone(), None, ColumnScope::Location);
                    }
                ) {
                    {{i18n::text("columns-restore")}}
                }
                DialogFooter {
                    DialogActions(
                        confirm_label = i18n::text("columns-apply"),
                        cancel_label = i18n::text("dialog-cancel"),
                        on_decision = UnsyncCallback::new(move |decision: DialogDecision| {
                            match decision {
                                DialogDecision::Confirm => browser.apply_columns(
                                    apply.clone(),
                                    Some(layout.get_untracked()),
                                    scope.get_untracked(),
                                ),
                                DialogDecision::Cancel | DialogDecision::Dismiss => {
                                    confirm.0.menus.close();
                                }
                            }
                        })
                    )
                }
            }
        })
    }

    fn apply_columns(&self, location: Location, layout: Option<Layout>, scope: ColumnScope) {
        self.update(|state| {
            match (scope, layout) {
                (_, None) => state.places.clear_columns(location),
                (ColumnScope::Location, Some(layout)) => state.places.set_columns(location, layout),
                (ColumnScope::Default, Some(layout)) => {
                    state.places.set_default_columns(layout);
                    state.places.clear_columns(location);
                }
            }
            for tab in &mut state.tabs {
                tab.columns = state
                    .places
                    .effective_columns(tab.navigation.location())
                    .clone();
                if !tab.columns.contains(tab.sort.key) {
                    tab.sort = Default::default();
                }
            }
        });
        self.save_places();
        self.0.menus.close();
        let tabs =
            self.read_untracked(|state| state.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>());
        for id in tabs {
            self.reproject(id);
            self.enrich_directory(id);
        }
    }
}
