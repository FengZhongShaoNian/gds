//! Files chrome keeps address navigation, folder filtering, and sidebar regions distinct.

use std::cell::Cell;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use gds::appearance::tokens::{SIDEBAR_ITEM_HEIGHT, SIDEBAR_PADDING};
use gds::i18n;
use gds::ui::button::{ButtonSize, IconButtonProps};
use gds::ui::capacity::CapacityBarProps;
use gds::ui::feedback::tooltip::{HintTooltipProps, wrap_tokens};
use gds::ui::frame::WindowDragAreaProps;
use gds::ui::interaction::{keytips, region};
use gds::ui::menu::{ContextMenuHostProps, MenuModel};
use gds::ui::navigation::{
    CornerActionProps, CornerButtonProps, NavItemContentProps, NavItemDetailsProps,
    NavItemLabelProps, NavItemProps, PinnedIconProps,
};
use gds::ui::sidebar::{
    SideBarContentProps, SideBarFooterProps, SideBarProps, SideBarSectionContentProps,
    SideBarSectionHeaderProps, SideBarSectionProps, SideBarSectionTitleProps, SidebarPresentation,
};
use gds::ui::{IconName, IconProps};
use zgui::geom::{DevicePx, Point};
use zgui::prelude::*;
use zgui::reactive::{RenderEffect, RwSignal, UnsyncCallback, on_cleanup_local};
use zgui::view;
use zgui::view::{ScrollBehavior, ScrollTarget, current_host, events};
use zgui::vocab::{Key, Modifiers, NamedKey, Orientation, PointerButton, Role, SharedString};
use zgui_ui::{InputProps, SpinnerProps};
use zgui_ui_primitives::Binding;

use super::keyboard::{SidebarRegion, SidebarRow};
use super::model::{AddressPresentation, Pane};
use super::ui::Browser;
use super::view::local_memo;
use crate::entities::file::ui::EntryPresentation;
use crate::entities::location::{
    Collection, Location, LocationTarget, scheme::TagAddress, volumes,
};
use crate::shared::ui::actions::Command;

const MIN_TABS_HEIGHT: f32 = 94.0;
const MIN_VOLUMES_HEIGHT: f32 = 108.0;
const SPLITTER_KEY_STEP: f32 = 16.0;

struct Breadcrumb {
    label: String,
    location: Location,
    current: bool,
}

impl Breadcrumb {
    fn for_location(location: &Location, path: Option<&Path>) -> Vec<Self> {
        if let Some(path) = path {
            let components = path
                .components()
                .filter(|part| !matches!(part, Component::CurDir))
                .collect::<Vec<_>>();
            let mut target = PathBuf::new();
            return components
                .iter()
                .enumerate()
                .map(|(index, part)| {
                    target.push(part.as_os_str());
                    let current = index + 1 == components.len();
                    let name = part.as_os_str().to_string_lossy();
                    let label =
                        if current || matches!(part, Component::RootDir | Component::Prefix(_)) {
                            name.into_owned()
                        } else {
                            format!("{name}/")
                        };
                    Self {
                        label,
                        location: Location::from(target.as_path()),
                        current,
                    }
                })
                .collect();
        }
        if let Location::Tags(address) = location {
            let mut result = vec![Self {
                label: i18n::text("tags-title"),
                location: Location::Tags(TagAddress::new("").expect("root tag address")),
                current: address.tag().is_empty(),
            }];
            let mut prefix = String::new();
            let parts = address
                .tag()
                .split('/')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            for (index, part) in parts.iter().enumerate() {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);
                let current = index + 1 == parts.len();
                result.push(Self {
                    label: (*part).to_owned(),
                    location: Location::Tags(TagAddress::new(&prefix).expect("valid tag ancestor")),
                    current,
                });
            }
            return result;
        }
        vec![Self {
            label: location.display_address(),
            location: location.clone(),
            current: true,
        }]
    }
}

impl Browser {
    pub(super) fn menu_controls(&self) -> AnyView {
        let browser = self.clone();
        AnyView::new(view! {
            row(class = "gds-files-menu-controls") {
                {{move || {
                    if browser.pane() == Pane::Files {
                        browser.app_menu()
                    } else {
                        let back = browser.clone();
                        let keytip_back = browser.clone();
                        let label = i18n::text("page-return");
                        AnyView::new(view! {
                            IconButton(
                                icon = IconName::ArrowLeft,
                                label = label,
                                keytip = Some("R"),
                                on_activate = UnsyncCallback::new(move |()| keytip_back.back_page()),
                                on:click = move |_| back.back_page()
                            )
                        })
                    }
                }}}
            }
        })
    }

    pub(super) fn toolbar(&self) -> AnyView {
        let browser = self.clone();
        let toggle_b = self.clone();
        let compact_b = self.clone();
        let toggle_browser = self.clone();
        let files_pane = self.clone();
        let sidebar_toggled = Signal::derive_local(move || toggle_browser.0.sidebar.get());
        let compact = Signal::derive_local(move || compact_b.compact());
        let is_files = Signal::derive_local(move || files_pane.pane() == Pane::Files);

        AnyView::new(view! {
            row(class = "gds-files-toolbar") {
                Show(when = move || !compact.get()) {
                    {{
                        let click_b = toggle_b.clone();
                        let act_b = toggle_b.clone();
                        view! {
                            IconButton(
                                icon = IconName::PanelLeft,
                                label = Signal::derive_local(|| i18n::text("sidebar-toggle")),
                                toggled = Some(sidebar_toggled),
                                shortcut = Some(Command::Sidebar.shortcut()),
                                keytip = Some("B"),
                                on_activate = UnsyncCallback::new(move |()| act_b.0.sidebar.update(|open| *open = !*open)),
                                on:click = move |_| click_b.0.sidebar.update(|open| *open = !*open)
                            )
                        }
                    }}
                }
                {{move || {
                    if is_files.get() {
                        AnyView::new(view! {
                            row(class = "gds-files-toolbar-files") {
                                {{browser.navigation_buttons()}}
                                {{browser.address_bar()}}
                                WindowDragArea(class = "gds-files-toolbar-spacer") { box() }
                            }
                        })
                    } else {
                        let title = match browser.pane() {
                            Pane::Settings => i18n::text("settings"),
                            Pane::Help => i18n::text("help"),
                            Pane::Files => String::new(),
                        };
                        AnyView::new(view! {
                            WindowDragArea(class = "gds-files-toolbar-title") {
                                text(class = "gds-files-toolbar-title-text") {{title}}
                            }
                        })
                    }
                }}}
            }
        })
    }

    fn navigation_buttons(&self) -> AnyView {
        let browser = self.clone();
        let state_sig = Signal::derive_local(move || {
            browser.read(|state| {
                let tab = state.tab();
                let history = tab.navigation.history();
                let index = tab.navigation.history_index();
                let backward = (0..index)
                    .rev()
                    .map(|i| (i, history[i].location.clone()))
                    .collect::<Vec<_>>();
                let forward = ((index + 1)..history.len())
                    .map(|i| (i, history[i].location.clone()))
                    .collect::<Vec<_>>();
                let can_up = matches!(
                    tab.navigation.location(),
                    Location::Tags(address) if !address.tag().is_empty()
                ) || tab.navigation.path().and_then(Path::parent).is_some();
                (tab.id, backward, forward, can_up)
            })
        });
        let parent_browser = self.clone();

        AnyView::new(view! {
            row(class = "gds-files-nav-group") {
                {{move || {
                    let (tab, backward, forward, can_up) = state_sig.get();
                    let b_click = parent_browser.clone();
                    let b_act = parent_browser.clone();
                    view! {
                        row(class = "gds-files-nav-group") {
                            {{parent_browser.history_button(tab, backward, false)}}
                            {{parent_browser.history_button(tab, forward, true)}}
                            IconButton(
                                icon = IconName::ArrowUp,
                                label = Signal::derive_local(|| i18n::text("parent")),
                                disabled = Signal::stored_local(!can_up),
                                shortcut = Some(Command::Parent.shortcut()),
                                keytip = Some("U"),
                                on_activate = UnsyncCallback::new(move |()| b_act.parent()),
                                on:click = move |_| b_click.parent()
                            )
                        }
                    }
                }}}
            }
        })
    }

    fn history_button(&self, tab: u64, visits: Vec<(usize, Location)>, forward: bool) -> AnyView {
        let disabled = visits.is_empty();
        let icon = if forward {
            IconName::ArrowRight
        } else {
            IconName::ArrowLeft
        };
        let label = i18n::text(if forward { "forward" } else { "back" });
        let shortcut = if forward {
            Command::Forward.shortcut()
        } else {
            Command::Back.shortcut()
        };
        let keytip = if forward { Some("AR") } else { Some("AL") };
        let click_browser = self.clone();
        let keytip_browser = self.clone();

        let button = AnyView::new(view! {
            IconButton(
                icon = icon,
                label = label,
                disabled = Signal::stored_local(disabled),
                shortcut = Some(shortcut),
                keytip = keytip,
                on_activate = UnsyncCallback::new(move |()| keytip_browser.history(forward)),
                on:click = move |_| click_browser.history(forward)
            )
        });
        if disabled {
            return button;
        }

        let menu_browser = self.clone();
        let model = std::rc::Rc::new(move || {
            let mut menu = MenuModel::new();
            for &(index, ref location) in &visits {
                let target_location = location.clone();
                let label = menu_browser.resolve_path(location).map_or_else(
                    || menu_browser.location_title(location),
                    |path| path.display().to_string(),
                );
                let weak = menu_browser.downgrade();
                menu.item(
                    label,
                    Some(location.icon()),
                    None,
                    false,
                    false,
                    UnsyncCallback::new(move |()| {
                        if let Some(browser) = weak.upgrade() {
                            browser.visit_history(tab, index, &target_location);
                            browser.focus_content_after_mount();
                        }
                    }),
                );
            }
            menu
        });

        AnyView::new(view! {
            ContextMenuHost(model = model) {{button}}
        })
    }

    fn address_bar(&self) -> AnyView {
        let editing = self.0.address_presentation;
        let is_editing = Signal::derive_local(move || {
            matches!(editing.get(), AddressPresentation::Editing { .. })
        });
        let destination = local_memo({
            let browser = self.clone();
            move || {
                browser.read(|state| {
                    let navigation = &state.tab().navigation;
                    let (location, path) = if let Some(pending) = navigation.pending_location() {
                        (pending, navigation.pending_path())
                    } else {
                        (navigation.location(), navigation.path())
                    };
                    (
                        location.clone(),
                        path.map(Path::to_owned),
                        navigation.is_loading(),
                    )
                })
            }
        });

        let edit_click = self.clone();
        let blur = self.clone();
        let submit = self.clone();
        let keys = self.clone();
        let crumbs_browser = self.clone();
        let edit_btn = self.clone();
        let composing = std::rc::Rc::new(Cell::new(false));
        let composing_start = std::rc::Rc::clone(&composing);
        let composing_preedit = std::rc::Rc::clone(&composing);
        let composing_commit = std::rc::Rc::clone(&composing);
        let composing_end = std::rc::Rc::clone(&composing);
        let address_sig = self.0.address;
        let address_node = self.0.address_node;

        AnyView::new(view! {
            row(
                class = "gds-files-address-bar",
                attr:data-editing = move || is_editing.get().then(|| "true".to_owned()),
                on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                    if ev.button == Some(PointerButton::Primary) {
                        ev.stop_propagation();
                    }
                },
                on:click = move |ev: &mut EventCx<'_, events::Click>| {
                    if !matches!(editing.get_untracked(), AddressPresentation::Editing { .. }) {
                        ev.stop_propagation();
                        edit_click.start_address_edit();
                    }
                }
            ) {
                {{move || {
                    if is_editing.get() {
                        let composing_key = std::rc::Rc::clone(&composing);
                        let submit_key = submit.clone();
                        let cancel_key = keys.clone();
                        let blur_out = blur.clone();
                        let cs = std::rc::Rc::clone(&composing_start);
                        let cp = std::rc::Rc::clone(&composing_preedit);
                        let cc = std::rc::Rc::clone(&composing_commit);
                        let ce = std::rc::Rc::clone(&composing_end);
                        AnyView::new(view! {
                            Input(
                                node_ref = address_node,
                                value = Binding::from(address_sig),
                                placeholder = i18n::text("path-placeholder"),
                                label = i18n::text("place-location"),
                                class = "gds-files-address-input",
                                on:ime_start = move |_| cs.set(true),
                                on:ime_preedit = move |ev: &mut EventCx<'_, events::ImePreedit>| cp.set(ev.is_composing()),
                                on:ime_commit = move |_| cc.set(false),
                                on:ime_end = move |_| ce.set(false),
                                on:focus_out = move |_| blur_out.finish_address_edit(),
                                on:key_down = move |ev: &mut EventCx<'_, events::KeyDown>| {
                                    if composing_key.get() {
                                        return;
                                    }
                                    if ev.key == Key::Named(NamedKey::Enter) && ev.modifiers == Modifiers::NONE {
                                        ev.prevent_default();
                                        ev.stop_propagation();
                                        submit_key.open_address();
                                    } else if ev.key == Key::Named(NamedKey::Escape) && ev.modifiers == Modifiers::NONE {
                                        ev.prevent_default();
                                        ev.stop_propagation();
                                        cancel_key.finish_address_edit();
                                        cancel_key.focus_content_after_mount();
                                    }
                                }
                            )
                        })
                    } else {
                        let (location, path, loading) = destination.get();
                        if loading {
                            let address = Self::location_address(&location, path.as_deref());
                            AnyView::new(view! {
                                row(class = "gds-files-address-loading") {
                                    text(class = "gds-files-address-loading-text") {{address}}
                                }
                            })
                        } else {
                            let b_click = edit_btn.clone();
                            let b_act = edit_btn.clone();
                            AnyView::new(view! {
                                row(class = "gds-files-address-read") {
                                    {{crumbs_browser.breadcrumbs(&location, path.as_deref())}}
                                    IconButton(
                                        icon = IconName::Pencil,
                                        label = Signal::derive_local(|| i18n::text("path-edit")),
                                        size = ButtonSize::IconSm,
                                        shortcut = Some(Command::EditLocation.shortcut()),
                                        keytip = Some("E"),
                                        on_activate = UnsyncCallback::new(move |()| b_act.start_address_edit()),
                                        on:click = move |ev: &mut EventCx<'_, events::Click>| {
                                            ev.stop_propagation();
                                            b_click.start_address_edit();
                                        }
                                    )
                                }
                            })
                        }
                    }
                }}}
            }
        })
    }

    fn breadcrumbs(&self, location: &Location, path: Option<&Path>) -> AnyView {
        let scroller = NodeRef::new();
        let size_obs = scroller.observe_border_size();
        let scroll_fx = RenderEffect::new(move |_| {
            let _ = size_obs.get();
            scroller.scroll_to(
                ScrollTarget::Offset(Point::new(DevicePx(f32::MAX), DevicePx(0.0))),
                ScrollBehavior::Instant,
            );
        });
        on_cleanup_local(move || drop(scroll_fx));

        let segments = Breadcrumb::for_location(location, path);
        let crumbs = segments
            .into_iter()
            .map(|crumb| {
                let open_click = self.clone();
                let target_loc = crumb.location.clone();
                let label = crumb.label;
                let a11y_label = label.clone();
                view! {
                    box(
                        class = "gds-files-crumb",
                        attr:data-current = crumb.current.then(|| "true".to_owned()),
                        tabindex = Focus::Sequential,
                        a11y:role = Role::Button,
                        a11y:label = a11y_label,
                        on:click = move |ev: &mut EventCx<'_, events::Click>| {
                            ev.stop_propagation();
                            open_click.open(target_loc.clone());
                            open_click.focus_content_after_mount();
                        }
                    ) {
                        text(class = "gds-files-crumb-text") {{label}}
                    }
                }
            })
            .collect::<Vec<_>>();

        let handle = request_frame(move |_| {
            scroller.scroll_to(
                ScrollTarget::Offset(Point::new(DevicePx(f32::MAX), DevicePx(0.0))),
                ScrollBehavior::Instant,
            );
        });
        on_cleanup_local(move || drop(handle));

        AnyView::new(view! {
            scroll(node_ref = scroller, class = "gds-files-breadcrumbs") {
                row(class = "gds-files-breadcrumbs-track") {
                    {crumbs}
                }
            }
        })
    }

    pub(super) fn start_address_edit(&self) {
        let value = self.read(|state| {
            let navigation = &state.tab().navigation;
            if let Some(pending) = navigation.pending_location() {
                Self::location_address(pending, navigation.pending_path())
            } else {
                Self::location_address(navigation.location(), navigation.path())
            }
        });
        self.0.address.set(value.clone());
        self.0
            .address_presentation
            .set(AddressPresentation::Editing {
                original: value.clone(),
            });
        let node = self.0.address_node;
        let weak = self.downgrade();
        let val = value;
        let handle = self.0.owner.with(|| {
            request_frame(move |_| {
                if weak.upgrade().is_some() && node.is_bound() {
                    node.set_value(&val);
                    node.focus();
                    node.select_all();
                }
            })
        });
        *self.0.pending_focus.borrow_mut() = Some(handle);
    }

    pub(super) fn finish_address_edit(&self) {
        let AddressPresentation::Editing { original } = self.0.address_presentation.get_untracked()
        else {
            return;
        };
        self.0
            .address_presentation
            .set(AddressPresentation::Breadcrumbs);
        self.0.address.set(original);
    }

    pub(super) fn folder_filter(&self) -> AnyView {
        let filter_sig = self.0.filter;
        let filter_node = self.0.filter_node;
        let available_browser = self.clone();
        let change_browser = self.clone();
        let clear_browser = self.clone();

        keytips::register_keytip(
            "F",
            filter_node,
            UnsyncCallback::new({
                let browser = self.clone();
                move |()| {
                    if !browser.filter_available() {
                        return;
                    }
                    filter_node.focus();
                    let val = filter_sig.get_untracked();
                    filter_node.set_value(&val);
                    filter_node.select_all();
                }
            }),
        );

        let has_value = Signal::derive_local(move || !filter_sig.get().is_empty());
        let visible = Signal::derive_local(move || available_browser.filter_available());

        AnyView::new(view! {
            Show(when = move || visible.get()) {
                {{
                    let change_browser = change_browser.clone();
                    let clear_browser = clear_browser.clone();
                    view! {
                        row(class = "gds-files-filter") {
                            Icon(name = IconName::Search, size = 14.0, class = "gds-files-filter-icon")
                            Input(
                                node_ref = filter_node,
                                value = Binding::from(filter_sig),
                                placeholder = i18n::text("filter-placeholder"),
                                label = i18n::text("filter"),
                                class = "gds-files-filter-input",
                                on_change = UnsyncCallback::new(move |value: String| {
                                    change_browser.filter(value);
                                })
                            )
                            Show(when = move || has_value.get()) {
                                {{
                                    let clear_b = clear_browser.clone();
                                    view! {
                                        IconButton(
                                            icon = IconName::Close,
                                            label = Signal::derive_local(|| i18n::text("close")),
                                            size = ButtonSize::IconXs,
                                            on:click = move |_| {
                                                filter_sig.set(String::new());
                                                clear_b.filter(String::new());
                                                filter_node.focus();
                                            }
                                        )
                                    }
                                }}
                            }
                        }
                    }
                }}
            }
        })
    }

    pub(super) fn filter_available(&self) -> bool {
        self.pane() == Pane::Files
            && self.read(|state| match state.tab().navigation.location() {
                Location::Recent | Location::Favorites | Location::Tags(_) => !self.compact(),
                Location::Remote(_) => self.compact(),
                Location::Directory(_) | Location::Volume { .. } => true,
                Location::Network | Location::Volumes | Location::Trash => false,
            })
    }

    pub(super) fn sidebar(&self, _presentation: SidebarPresentation) -> AnyView {
        let collapsed = Signal::derive_local({
            let browser = self.clone();
            move || !browser.sidebar_expanded()
        });

        let browser = self.clone();
        let sections = local_memo({
            let browser = self.clone();
            move || {
                let preferences = browser.0.preferences.snapshot();
                (
                    preferences.files.sidebar.show_volumes
                        || browser.0.sidebar_reveal_volumes.get(),
                    preferences.tags.enabled,
                )
            }
        });

        keytips::register_keytip(
            "T",
            self.0.tabs_node,
            UnsyncCallback::new({
                let b = self.clone();
                move |()| b.0.tabs_node.focus()
            }),
        );
        keytips::register_keytip(
            "V",
            self.0.volumes_node,
            UnsyncCallback::new({
                let b = self.clone();
                move |()| b.0.volumes_node.focus()
            }),
        );
        keytips::register_keytip(
            "L",
            self.0.locations_node,
            UnsyncCallback::new({
                let b = self.clone();
                move |()| b.0.locations_node.focus()
            }),
        );

        let content_ref = NodeRef::new();
        let content_size = content_ref.observe_border_size();
        let height_sig = self.0.sidebar_tabs_height;
        let drag_start: RwSignal<Option<(f32, f32)>, LocalStorage> = RwSignal::new_local(None);
        let window = try_use_window();

        let available_height = Signal::derive_local(move || {
            let scale = content_ref.scale().max(1.0);
            content_size
                .get()
                .map_or(420.0, |sz| (sz.height.0 / scale - 12.0).max(0.0))
        });

        let clamp_tabs = move |preferred: Option<f32>| {
            let usable = (available_height.get_untracked() - 9.0).max(0.0);
            let minimum = MIN_TABS_HEIGHT.min(usable * 0.5);
            let maximum = (usable - MIN_VOLUMES_HEIGHT).max(minimum);
            preferred.unwrap_or(usable * 0.5).clamp(minimum, maximum)
        };

        let tabs_flex_style = Signal::derive_local({
            let browser = browser.clone();
            move || {
                let (show_volumes, _) = sections.get();
                let volumes_collapsed = browser
                    .0
                    .preferences
                    .snapshot()
                    .files
                    .sidebar
                    .volumes_collapsed
                    && browser.0.sidebar_region.get() != Some(SidebarRegion::Volumes);
                if !show_volumes || volumes_collapsed {
                    return None;
                }
                let usable = (available_height.get() - 9.0).max(0.0);
                let minimum = MIN_TABS_HEIGHT.min(usable * 0.5);
                let maximum = (usable - MIN_VOLUMES_HEIGHT).max(minimum);
                let h = height_sig
                    .get()
                    .unwrap_or(usable * 0.5)
                    .clamp(minimum, maximum);
                Some(format!("{h:.1}px"))
            }
        });

        let new_tab_browser = browser.clone();
        let volumes_toggle = browser.clone();
        let locations_toggle = browser.clone();
        let persist_drag = browser.clone();
        let persist_cancel = browser.clone();
        let persist_key = browser.clone();
        let volumes_browser = browser.clone();
        let tabs_browser = browser.clone();
        let locs_browser = browser.clone();
        let tabs_node = self.0.tabs_node;
        let volumes_node = self.0.volumes_node;
        let locations_node = self.0.locations_node;

        AnyView::new(view! {
            SideBar(collapsed = collapsed, label = i18n::text("locations")) {
                column(node_ref = content_ref, class = "gds-files-sidebar-content-wrap") {
                SideBarContent(class = "gds-files-sidebar-content") {
                    SideBarSection(
                        class = "gds-files-sidebar-tabs-section",
                        attr:data-fixed-height = move || tabs_flex_style.get().is_some().then(|| "true".to_owned()),
                        style:height = move || tabs_flex_style.get()
                    ) {
                        SideBarSectionHeader {
                            SideBarSectionTitle {{i18n::text("tabs")}}
                            IconButton(
                                icon = IconName::Plus,
                                label = Signal::derive_local(|| i18n::text("new-tab")),
                                size = ButtonSize::IconSm,
                                shortcut = Some(Command::NewTab.shortcut()),
                                keytip = Some("N"),
                                on_activate = UnsyncCallback::new({
                                    let b = new_tab_browser.clone();
                                    move |()| {
                                        b.new_tab(None);
                                        b.focus_content_after_mount();
                                    }
                                }),
                                on:click = {
                                    let b = new_tab_browser.clone();
                                    move |_| {
                                        b.new_tab(None);
                                        b.focus_content_after_mount();
                                    }
                                }
                            )
                        }
                        SideBarSectionContent(class = "gds-files-sidebar-region-host") {
                            scroll(
                                node_ref = tabs_node,
                                class = "gds-files-sidebar-scroll",
                                {..region::surface(tabs_node)},
                                on:focus_in = {
                                    let b = browser.clone();
                                    move |_| b.sidebar_focus_changed(SidebarRegion::Tabs, true)
                                },
                                on:focus_out = {
                                    let b = browser.clone();
                                    move |_| {
                                        let inside = current_host().is_some_and(|host| {
                                            host.focused().get_untracked().is_some_and(|active| {
                                                tabs_node.get_untracked() == Some(active)
                                                    || tabs_node.contains(active)
                                            })
                                        });
                                        if !inside {
                                            b.sidebar_focus_changed(SidebarRegion::Tabs, false);
                                        }
                                    }
                                }
                            ) {
                                {{move || {
                                    let tabs = tabs_browser.read(|state| {
                                        state
                                            .tabs
                                            .iter()
                                            .map(|tab| {
                                                (
                                                    tab.id,
                                                    tab.navigation.location().clone(),
                                                    tab.navigation.path().map(Path::to_owned),
                                                    tab.pinned,
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                    });
                                    let collapsed_now = collapsed.get();
                                    tabs.into_iter()
                                        .map(|(id, location, local_path, pinned)| {
                                            tabs_browser.sidebar_tab(
                                                id,
                                                location,
                                                local_path,
                                                pinned,
                                                collapsed_now,
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                }}}
                            }
                        }
                    }

                    Show(when = move || {
                        let (show_volumes, _) = sections.get();
                        let volumes_collapsed = volumes_toggle
                            .0
                            .preferences
                            .snapshot()
                            .files
                            .sidebar
                            .volumes_collapsed
                            && volumes_toggle.0.sidebar_region.get() != Some(SidebarRegion::Volumes);
                        show_volumes && !volumes_collapsed && !collapsed.get()
                    }) {
                        {{
                            let persist_drag = persist_drag.clone();
                            let persist_cancel = persist_cancel.clone();
                            let persist_key = persist_key.clone();
                            let win_enter = window.clone();
                            let win_leave = window.clone();
                            view! {
                                control(
                                    class = "gds-files-sidebar-splitter",
                                    tabindex = Focus::Sequential,
                                    a11y:role = Role::Splitter,
                                    a11y:orientation = Orientation::Vertical,
                                    a11y:label = i18n::text("sidebar-resize"),
                                    on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                                        if ev.button != Some(PointerButton::Primary) || !ev.primary {
                                            return;
                                        }
                                        let current = clamp_tabs(height_sig.get_untracked());
                                        drag_start.set(Some((ev.position.y.0, current)));
                                        ev.capture_pointer();
                                        ev.stop_propagation();
                                    },
                                    on:pointer_move = move |ev: &mut EventCx<'_, events::PointerMove>| {
                                        let Some((start_y, start_h)) = drag_start.get_untracked() else {
                                            return;
                                        };
                                        let delta = ev.position.y.0 - start_y;
                                        height_sig.set(Some(clamp_tabs(Some(start_h + delta))));
                                    },
                                    on:pointer_up = {
                                        let p = persist_drag.clone();
                                        move |ev: &mut EventCx<'_, events::PointerUp>| {
                                            if drag_start.get_untracked().is_some() {
                                                drag_start.set(None);
                                                ev.release_pointer();
                                                p.persist_sidebar_height();
                                            }
                                        }
                                    },
                                    on:pointer_cancel = {
                                        let p = persist_cancel.clone();
                                        move |ev: &mut EventCx<'_, events::PointerCancel>| {
                                            if drag_start.get_untracked().is_some() {
                                                drag_start.set(None);
                                                ev.release_pointer();
                                                p.persist_sidebar_height();
                                            }
                                        }
                                    },
                                    on:pointer_enter = {
                                        let win = win_enter.clone();
                                        move |_| {
                                            if let Some(ref w) = win {
                                                w.set_cursor(CursorStyle::ResizeNorthSouth);
                                            }
                                        }
                                    },
                                    on:pointer_leave = {
                                        let win = win_leave.clone();
                                        move |_| {
                                            if drag_start.get_untracked().is_none()
                                                && let Some(ref w) = win
                                            {
                                                w.set_cursor(CursorStyle::Default);
                                            }
                                        }
                                    },
                                    on:key_down = {
                                        let p = persist_key.clone();
                                        move |ev: &mut EventCx<'_, events::KeyDown>| {
                                            if !ev.modifiers.is_empty() {
                                                return;
                                            }
                                            let step = match ev.key {
                                                Key::Named(NamedKey::ArrowUp) => -SPLITTER_KEY_STEP,
                                                Key::Named(NamedKey::ArrowDown) => SPLITTER_KEY_STEP,
                                                _ => return,
                                            };
                                            let next = clamp_tabs(Some(clamp_tabs(height_sig.get_untracked()) + step));
                                            height_sig.set(Some(next));
                                            p.persist_sidebar_height();
                                            ev.prevent_default();
                                            ev.stop_propagation();
                                        }
                                    }
                                ) {
                                    box(class = "gds-files-sidebar-splitter-line", a11y:hidden = true) {}
                                }
                            }
                        }}
                    }

                    Show(when = move || sections.get().0) {
                        {{
                            let vol_collapsed = Signal::derive_local({
                                let b = volumes_browser.clone();
                                move || {
                                    b.0.preferences.snapshot().files.sidebar.volumes_collapsed
                                        && b.0.sidebar_region.get() != Some(SidebarRegion::Volumes)
                                }
                            });
                            let active_volume = local_memo({
                                let b = volumes_browser.clone();
                                move || {
                                    b.read(|state| match state.tab().navigation.location() {
                                        Location::Volume { identity, .. } => {
                                            b.volume_at_location(identity).map(|volume| volume.id)
                                        }
                                        _ => None,
                                    })
                                }
                            });
                            let b_toggle = volumes_browser.clone();
                            let b_toggle_pref = volumes_browser.clone();
                            let b_rows = volumes_browser.clone();
                            let b_focus = volumes_browser.clone();

                            view! {
                                SideBarSection(
                                    class = "gds-files-sidebar-volumes-section",
                                    attr:data-section-collapsed = move || vol_collapsed.get().then(|| "true".to_owned())
                                ) {
                                    {{b_toggle.sidebar_heading("volumes", vol_collapsed, move || {
                                        b_toggle_pref.edit_preferences(|p| {
                                            p.files.sidebar.volumes_collapsed = !p.files.sidebar.volumes_collapsed;
                                        });
                                    })}}
                                    Show(when = move || !vol_collapsed.get()) {
                                        {{
                                            let b_focus = b_focus.clone();
                                            let b_rows = b_rows.clone();
                                            view! {
                                                SideBarSectionContent(class = "gds-files-sidebar-region-host") {
                                                    scroll(
                                                        node_ref = volumes_node,
                                                        class = "gds-files-sidebar-scroll",
                                                        {..region::surface(volumes_node)},
                                                        on:focus_in = {
                                                            let b = b_focus.clone();
                                                            move |_| b.sidebar_focus_changed(SidebarRegion::Volumes, true)
                                                        },
                                                        on:focus_out = {
                                                            let b = b_focus.clone();
                                                            move |_| {
                                                                let inside = current_host().is_some_and(|host| {
                                                                    host.focused().get_untracked().is_some_and(|active| {
                                                                        volumes_node.get_untracked() == Some(active)
                                                                            || volumes_node.contains(active)
                                                                    })
                                                                });
                                                                if !inside {
                                                                    b.sidebar_focus_changed(SidebarRegion::Volumes, false);
                                                                }
                                                            }
                                                        }
                                                    ) {
                                                        {{move || {
                                                            let snapshot = b_rows.0.volumes.snapshot();
                                                            let compact_now = collapsed.get();
                                                            if snapshot.volumes.is_empty() {
                                                                vec![AnyView::new(view! {
                                                                    text(class = "gds-files-sidebar-empty") {{i18n::text("device-none")}}
                                                                })]
                                                            } else {
                                                                snapshot
                                                                    .volumes
                                                                    .iter()
                                                                    .map(|volume| {
                                                                        b_rows.sidebar_volume(
                                                                            volume,
                                                                            &snapshot,
                                                                            active_volume,
                                                                            compact_now,
                                                                        )
                                                                    })
                                                                    .collect::<Vec<_>>()
                                                            }
                                                        }}}
                                                    }
                                                }
                                            }
                                        }}
                                    }
                                }
                            }
                        }}
                    }
                }
                }

                SideBarFooter(class = "gds-files-sidebar-footer") {
                    {{
                        let loc_collapsed = Signal::derive_local({
                            let b = locs_browser.clone();
                            move || {
                                b.0.preferences.snapshot().files.sidebar.locations_collapsed
                                    && b.0.sidebar_region.get() != Some(SidebarRegion::Locations)
                            }
                        });
                        let b_toggle = locations_toggle.clone();
                        let b_toggle_pref = locations_toggle.clone();
                        let b_locs = locs_browser.clone();

                        view! {
                            SideBarSection {
                                {{b_toggle.sidebar_heading("locations", loc_collapsed, move || {
                                    b_toggle_pref.edit_preferences(|p| {
                                        p.files.sidebar.locations_collapsed = !p.files.sidebar.locations_collapsed;
                                    });
                                })}}
                                Show(when = move || !loc_collapsed.get()) {
                                    {{
                                        let b_locs = b_locs.clone();
                                        view! {
                                            column(
                                                node_ref = locations_node,
                                                class = "gds-files-sidebar-locations",
                                                {..region::surface(locations_node)},
                                                on:focus_in = {
                                                    let b = b_locs.clone();
                                                    move |_| b.sidebar_focus_changed(SidebarRegion::Locations, true)
                                                },
                                                on:focus_out = {
                                                    let b = b_locs.clone();
                                                    move |_| {
                                                        let inside = current_host().is_some_and(|host| {
                                                            host.focused().get_untracked().is_some_and(|active| {
                                                                locations_node.get_untracked() == Some(active)
                                                                    || locations_node.contains(active)
                                                            })
                                                        });
                                                        if !inside {
                                                            b.sidebar_focus_changed(SidebarRegion::Locations, false);
                                                        }
                                                    }
                                                }
                                            ) {
                                                {{move || {
                                                    let tags_enabled = sections.get().1;
                                                    let home = b_locs.read(|state| state.home.clone());
                                                    LocationTarget::ALL
                                                        .iter()
                                                        .copied()
                                                        .filter(|target| {
                                                            tags_enabled
                                                                || !matches!(
                                                                    target,
                                                                    LocationTarget::Collection(Collection::Tags)
                                                                )
                                                        })
                                                        .map(|target| {
                                                            let loc = target.location(&home);
                                                            let item = b_locs.sidebar_location(target, loc.clone());
                                                            match loc.local_path() {
                                                                Some(path) => b_locs.file_drag_target(item, path.to_owned()),
                                                                None => item,
                                                            }
                                                        })
                                                        .collect::<Vec<_>>()
                                                }}}
                                            }
                                        }
                                    }}
                                }
                            }
                        }
                    }}
                }
            }
        })
    }

    fn persist_sidebar_height(&self) {
        let height = self.0.sidebar_tabs_height.get_untracked();
        self.edit_preferences(|preferences| preferences.files.sidebar.tabs_height = height);
    }

    fn sidebar_heading(
        &self,
        label: &'static str,
        collapsed: Signal<bool, LocalStorage>,
        toggle: impl Fn() + 'static,
    ) -> AnyView {
        AnyView::new(view! {
            SideBarSectionHeader {
                row(
                    class = "gds-files-sidebar-heading",
                    tabindex = Focus::Sequential,
                    a11y:role = Role::Button,
                    a11y:label = move || SharedString::from(i18n::text(label)),
                    a11y:expanded = move || !collapsed.get(),
                    attr:data-collapsed = move || collapsed.get().then(|| "true".to_owned()),
                    on:click = move |_| toggle()
                ) {
                    Icon(
                        name = IconName::ChevronRight,
                        size = 12.0,
                        class = "gds-files-sidebar-chevron"
                    )
                    SideBarSectionTitle {{move || i18n::text(label)}}
                }
            }
        })
    }

    fn sidebar_volume(
        &self,
        volume: &volumes::Volume,
        snapshot: &volumes::Snapshot,
        active: Signal<Option<volumes::VolumeId>, LocalStorage>,
        compact: bool,
    ) -> AnyView {
        let icon = if volume.locked {
            IconName::Lock
        } else {
            match volume
                .drive
                .as_deref()
                .and_then(|id| snapshot.drive(id))
                .map(|drive| drive.kind)
            {
                Some(volumes::DeviceKind::Usb) => IconName::Usb,
                Some(volumes::DeviceKind::Optical) => IconName::Disc,
                _ => IconName::HardDrive,
            }
        };

        let action = self.clone();
        let device = volume.id.clone();
        let candidate = SidebarRow::Volume(volume.id.clone());
        let cursor = self.0.sidebar_cursor;
        let selected = Signal::derive_local({
            let id = volume.id.clone();
            move || active.get().as_ref() == Some(&id)
        });
        let cursor_match =
            Signal::derive_local(move || cursor.with(|row| row.as_ref() == Some(&candidate)));

        let status = self.0.volumes.capacity(&volume.id);
        let volume_busy = self.0.volumes.is_busy(&volume.id);
        let mut hint = vec![volume.name.clone(), volume.filesystem.clone()];
        if volume.system_mount {
            hint.push(i18n::text("device-system-volume"));
        }

        let details = if volume.mount_points.is_empty() {
            AnyView::new(view! {
                text(class = "gds-files-volume-unmounted") {{i18n::text("device-not-mounted")}}
            })
        } else {
            let (summary, fraction, unavailable) = match status {
                Some(volumes::CapacityState::Ready(capacity)) => {
                    let available = EntryPresentation::format_size(capacity.available);
                    let total = EntryPresentation::format_size(capacity.total);
                    (
                        gds::t!("device-capacity", available = available, total = total),
                        Some(capacity.unavailable_fraction()),
                        false,
                    )
                }
                Some(volumes::CapacityState::Unavailable(error)) => {
                    hint.push(error);
                    (i18n::text("device-capacity-unavailable"), None, true)
                }
                _ => (i18n::text("device-capacity-loading"), None, false),
            };
            hint.push(summary.clone());
            AnyView::new(view! {
                box(class = "gds-files-volume-capacity") {
                    CapacityBar(
                        label = summary,
                        fraction = fraction,
                        compact = compact,
                        unavailable = unavailable,
                        selected = selected
                    )
                }
            })
        };

        let name = volume.name.clone();
        let a11y_label = volume.name.clone();
        let tooltip_text = wrap_tokens(&hint.join("\n"));

        let item = AnyView::new(view! {
            NavItem(
                selected = selected,
                class = "gds-files-volume-item",
                attr:data-cursor = move || cursor_match.get().then(|| "true".to_owned()),
                a11y:label = a11y_label,
                on:click = move |ev: &mut EventCx<'_, events::Click>| {
                    action.open_volume(&device, ev.modifiers.primary());
                    action.focus_content_after_mount();
                }
            ) {
                column(class = "gds-files-volume-inner") {
                    row(class = "gds-files-volume-header") {
                        Icon(name = icon, size = 18.0, class = "gds-files-nav-icon")
                        NavItemLabel {{name}}
                        {{volume_busy.then(|| AnyView::new(view! { Spinner(class = "gds-files-volume-spinner") }))}}
                    }
                    {details}
                }
            }
        });

        let tipped = AnyView::new(view! {
            HintTooltip(text = tooltip_text) {{item}}
        });
        self.sidebar_volume_menu(tipped, volume)
    }

    fn sidebar_location(&self, target: LocationTarget, location: Location) -> AnyView {
        let label = i18n::text(target.label_id());
        let icon = target.icon();
        let cursor = self.0.sidebar_cursor;
        let open = self.clone();
        let selected = Signal::derive_local({
            let browser = self.clone();
            let location = location.clone();
            move || {
                browser.read(|state| {
                    let navigation = &state.tab().navigation;
                    navigation.location() == &location
                        || location
                            .local_path()
                            .is_some_and(|path| navigation.path() == Some(path))
                })
            }
        });
        let cursor_match = Signal::derive_local(move || {
            cursor.with(|row| row.as_ref() == Some(&SidebarRow::Location(target)))
        });

        let accessible = label.clone();
        let tooltip = label.clone();

        AnyView::new(view! {
            HintTooltip(text = tooltip) {
                NavItem(
                    selected = selected,
                    class = "gds-files-location-item",
                    attr:data-cursor = move || cursor_match.get().then(|| "true".to_owned()),
                    style:height = format!("{SIDEBAR_ITEM_HEIGHT}px"),
                    style:padding = format!("0px {SIDEBAR_PADDING}px"),
                    a11y:label = accessible,
                    on:click = move |_| {
                        open.open(location.clone());
                        open.focus_content_after_mount();
                    }
                ) {
                    Icon(name = icon, size = 18.0, class = "gds-files-nav-icon")
                    NavItemLabel {{label}}
                }
            }
        })
    }

    fn sidebar_tab(
        &self,
        id: u64,
        location: Location,
        local_path: Option<PathBuf>,
        pinned: bool,
        compact: bool,
    ) -> AnyView {
        let title = Signal::derive_local({
            let browser = self.clone();
            let location = location.clone();
            move || browser.location_title(&location)
        });
        let path = Self::location_address(&location, local_path.as_deref());
        let tooltip = wrap_tokens(&path);
        let select = self.clone();
        let action = self.clone();
        let middle_close = self.clone();
        let hovered = RwSignal::new_local(false);
        let middle_pressed = Rc::new(Cell::new(false));
        let middle_down = Rc::clone(&middle_pressed);
        let middle_up = Rc::clone(&middle_pressed);
        let cursor = self.0.sidebar_cursor;
        let selected = Signal::derive_local({
            let browser = self.clone();
            move || browser.read(|state| state.tab().id == id)
        });
        let cursor_match = Signal::derive_local(move || {
            cursor.with(|row| row.as_ref() == Some(&SidebarRow::Tab(id)))
        });
        let action_revealed = Signal::derive_local(move || (pinned || hovered.get()) && !compact);
        let corner_icon = if pinned {
            IconName::Pin
        } else {
            IconName::Close
        };
        let corner_label = i18n::text(if pinned {
            "unpin-tab"
        } else {
            "close-this-tab"
        });

        let item = AnyView::new(view! {
            NavItem(
                selected = selected,
                class = "gds-files-tab-item",
                attr:data-cursor = move || cursor_match.get().then(|| "true".to_owned()),
                a11y:label = move || SharedString::from(title.get()),
                on:pointer_enter = move |_| hovered.set(true),
                on:pointer_leave = move |_| hovered.set(false),
                on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                    if ev.button == Some(PointerButton::Middle) {
                        middle_down.set(true);
                        ev.stop_propagation();
                    }
                },
                on:pointer_up = move |ev: &mut EventCx<'_, events::PointerUp>| {
                    if middle_up.replace(false) && ev.button == Some(PointerButton::Middle) {
                        ev.stop_propagation();
                        if let Some(index) = middle_close.tab_index(id) {
                            middle_close.close_tab(index);
                        }
                    }
                },
                on:click = move |_| {
                    if let Some(index) = select.tab_index(id) {
                        select.select_tab(index);
                        select.focus_content_after_mount();
                    }
                }
            ) {
                PinnedIcon(icon = location.icon(), pinned = pinned)
                NavItemContent {
                    NavItemLabel {{move || title.get()}}
                    NavItemDetails {{path}}
                }
                CornerAction(
                    revealed = action_revealed,
                    button = AnyView::new(view! {
                        CornerButton(
                            icon = corner_icon,
                            label = corner_label,
                            on:click = move |_| {
                                if let Some(index) = action.tab_index(id) {
                                    if pinned {
                                        action.toggle_pin(index);
                                        action.focus_content_after_mount();
                                    } else {
                                        action.close_tab(index);
                                    }
                                }
                            }
                        )
                    })
                )
            }
        });

        let tipped = AnyView::new(view! {
            HintTooltip(text = tooltip) {{item}}
        });
        let wrapped = match local_path {
            Some(path) => self.file_drag_target(tipped, path),
            None => tipped,
        };
        let dragged = self.draggable_tab(wrapped, id, pinned);
        self.tab_menu(dragged, id)
    }
}
