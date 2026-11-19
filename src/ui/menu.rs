//! Declarative and model-driven popup / context menus wrapping `zgui-ui` (`shortcuts.md` §3).

use zgui::geom::{Css, CssPx, Point};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, StoredValue, UnsyncCallback};
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

use crate::ui::icon::{IconName, IconProps};
use crate::ui::{Shortcut, ShortcutBadgeProps};

/// Preferred default width (in CSS px) of a popup or context menu surface.
pub const WIDTH: f32 = 320.0;
/// Minimum width (in CSS px) of a popup or context menu surface (`main:menu.rs`).
pub const MIN_WIDTH: f32 = 220.0;
/// Maximum width (in CSS px) of a popup or context menu surface (`main:menu.rs`).
pub const MAX_WIDTH: f32 = 500.0;
/// Maximum height (in CSS px) of a compact dropdown popup menu surface (`main:menu.rs`).
pub const MAX_HEIGHT: f32 = 360.0;
/// Window edge margin (in CSS px) reserved around context menus (`main:menu.rs`).
pub const EDGE_MARGIN: f32 = 8.0;
/// Surface border thickness (in CSS px) around popup and context menus (`main:menu.rs`).
pub const SURFACE_BORDER: f32 = 1.0;
/// Horizontal gap (in CSS px) between an item label and its trailing [`ShortcutBadge`].
pub const SHORTCUT_GAP: f32 = 24.0;

const SHEET: &str = css!(
    r#"
    .gds-menu-content,
    .gds-menu-content.gds-menu-content {
        --zui-surface-fill: var(--gds-panel);
        --zui-surface-ink: var(--gds-panel-foreground);
        --zui-surface-border: 1px solid var(--gds-border);
        --zui-surface-radius: 8px;
        background-color: var(--gds-panel);
        color: var(--gds-panel-foreground);
        border: 1px solid var(--gds-border);
        border-radius: 8px;
        min-width: 220px;
        max-width: 500px;
        max-height: min(360px, var(--zui-popper-available-height, 360px));
        overflow-x: hidden;
        overflow-y: auto;
        font-family: var(--gds-font-sans);
        font-size: var(--gds-font-size-small, 14px);
    }

    .gds-menu-content.gds-menu-content.gds-menu-content--context {
        max-height: min(calc(100vh - 18px), var(--zui-popper-available-height, calc(100vh - 18px)));
    }

    .gds-menu-content .zui-menu__item,
    .gds-menu-content .zui-menu__label {
        font-size: var(--gds-font-size-small, 14px);
    }

    .gds-menu-content .zui-menu__item:not(.zui-menu__item--destructive):hover,
    .gds-menu-content .zui-menu__item:not(.zui-menu__item--destructive):focus,
    .gds-menu-content .zui-menu__item:not(.zui-menu__item--destructive)[data-state="open"] {
        background-color: var(--gds-hover);
        color: inherit;
    }

    .gds-menu-host {
        display: contents;
    }

    .gds-menu-item-body {
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        flex: 1 1 auto;
        width: 100%;
        min-width: 0;
        gap: 24px;
        font-size: var(--gds-font-size-small, 14px);
    }

    .gds-menu-item-row {
        display: inline-flex;
        align-items: center;
        flex: 1 1 auto;
        gap: 8px;
        min-width: 0;
    }

    .gds-menu-item-label {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: var(--gds-font-size-small, 14px);
    }

    .gds-menu-item-shortcut {
        display: inline-flex;
        align-items: center;
        flex-shrink: 0;
        margin-left: auto;
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

/// Renders a slice of [`MenuEntry`] values into `zgui-ui` menu items with `22px` trailing
/// [`ShortcutBadge`] keycaps (`24px` gap, `14px` text) and AT-SPI shortcut metadata.
#[must_use]
pub fn render_entries(entries: Vec<MenuEntry>) -> Vec<AnyView> {
    install_menu_stylesheet();
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
                let inset = align_column && icon.is_none();
                let mut item_attrs = Attrs::new();
                if let Some(ref key) = shortcut {
                    let hint = zgui::vocab::SharedString::from(key.label());
                    item_attrs =
                        item_attrs.a11y_from(A11yBinding::unspecified().keyboard_shortcut(hint));
                }
                AnyView::new(view! {
                    MenuItem(
                        disabled = disabled,
                        destructive = destructive,
                        inset = inset,
                        on_select = on_select,
                        {..item_attrs}
                    ) {
                        row(class = "gds-menu-item-body") {
                            row(class = "gds-menu-item-row") {
                                {icon.map(|name| view! { Icon(name = name) }.into_view())}
                                text(class = "gds-menu-item-label") {{label}}
                            }
                            {shortcut.map(|key| {
                                view! {
                                    box(class = "gds-menu-item-shortcut", a11y:hidden = true) {
                                        ShortcutBadge(shortcut = key)
                                    }
                                }
                                .into_view()
                            })}
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
                let sub_entries = StoredValue::new_local(entries);
                AnyView::new(view! {
                    MenuSub {
                        MenuSubTrigger(inset = inset) {
                            row(class = "gds-menu-item-row") {
                                {icon.map(|name| view! { Icon(name = name) }.into_view())}
                                text(class = "gds-menu-item-label") {{label}}
                            }
                        }
                        MenuSubContent(class = "gds-menu-content gds-menu-content--context") {
                            {sub_entries.with_value(|list| render_entries(list.clone()))}
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

pub(crate) fn install_menu_stylesheet() {
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
            ContextMenuContent(class = "gds-menu-content gds-menu-content--context") {
                {render_entries(build().entries)}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};
    use std::rc::Rc;
    use std::sync::Arc;

    use zgui::app::Fonts;
    use zgui::atlas::{MemorySink, TextureSink};
    use zgui::bits::DamageSet;
    use zgui::geom::{CssPx, DevicePx, Point, Size};
    use zgui::platform::{AppHandler, PlatformError, Surface, SurfaceEvent};
    use zgui::prelude::*;
    use zgui::reactive::{Mounted, RwSignal, UnsyncCallback, flush, install};
    use zgui::render::{
        ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
        Renderer, TextureHandle,
    };
    use zgui::runtime::AppError;
    use zgui::scene::Scene;
    use zgui::view;
    use zgui::vocab::{Modifiers, PointerAction, PointerButton, PointerEvent, Role, Timestamp};
    use zgui_platform_headless::{Harness, OffscreenSurface};

    use super::{
        ContextMenuHostProps, EDGE_MARGIN, MAX_HEIGHT, MAX_WIDTH, MIN_WIDTH, MenuEntry, MenuModel,
        PopupMenuProps, SHEET, SHORTCUT_GAP, SURFACE_BORDER, has_leading_column, popup_in,
        render_entries,
    };
    use crate::app::Gds;
    use crate::service::task_manager::Scope;
    use crate::ui::Shortcut;
    use crate::ui::icon::IconName;

    #[derive(Default)]
    struct NoopRenderer {
        atlas: MemorySink,
        target: Option<RenderTarget>,
    }

    impl Renderer for NoopRenderer {
        fn capabilities(&self) -> RenderCapabilities {
            RenderCapabilities::MINIMAL
        }

        fn configure(&mut self, target: RenderTarget) {
            self.target = Some(target);
        }

        fn target(&self) -> Option<RenderTarget> {
            self.target
        }

        fn register_external(&mut self, _: ExternalTexture) -> TextureHandle {
            TextureHandle(0)
        }

        fn release_external(&mut self, _: TextureHandle) {}

        fn memory(&self) -> MemoryReport {
            MemoryReport::default()
        }

        fn draw(&mut self, _: &Scene, _: &DamageSet) -> FrameOutcome {
            FrameOutcome::Presented(FrameStats::default())
        }

        fn texture_sink(&mut self) -> &mut dyn TextureSink {
            &mut self.atlas
        }
    }

    fn noop_renderer(
        _: &Arc<dyn Surface>,
        target: RenderTarget,
    ) -> Result<Box<dyn Renderer>, AppError> {
        let mut renderer = NoopRenderer::default();
        renderer.configure(target);
        Ok(Box::new(renderer))
    }

    fn live_menu_heights(surface: &OffscreenSurface) -> Vec<f64> {
        let mut nodes = HashMap::new();
        let mut root = None;
        for update in surface.a11y_log() {
            if let Some(tree) = update.tree {
                root = Some(tree.root);
            }
            for (id, node) in update.nodes {
                nodes.insert(id, node);
            }
        }
        let Some(root_id) = root else {
            return Vec::new();
        };
        let mut stack = vec![root_id];
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(node) = nodes.get(&id) else {
                continue;
            };
            if node.role() == Role::Menu
                && let Some(rect) = node.bounds()
            {
                out.push(rect.y1 - rect.y0);
            }
            for &child in node.children().iter().rev() {
                stack.push(child);
            }
        }
        out
    }

    #[test]
    fn menu_dimensions_and_entry_rendering_match_spec() {
        assert_eq!(MIN_WIDTH, 220.0);
        assert_eq!(MAX_WIDTH, 500.0);
        assert_eq!(MAX_HEIGHT, 360.0);
        assert_eq!(2.0 * (EDGE_MARGIN + SURFACE_BORDER), 18.0);
        assert_eq!(SHORTCUT_GAP, 24.0);
        assert!(
            SHEET.contains("max-height: min(360px, var(--zui-popper-available-height, 360px));"),
            "compact dropdown menus must stay capped at 360px"
        );
        assert!(
            SHEET.contains(
                ".gds-menu-content.gds-menu-content.gds-menu-content--context {\n        max-height: min(calc(100vh - 18px), var(--zui-popper-available-height, calc(100vh - 18px)));"
            ),
            "context menus must expand up to available window/popper height (100vh - 18px)"
        );

        install().ok();
        let scope = Mounted::new();
        scope.with(|| {
            let mut plain = MenuModel::new();
            plain.item(
                "Rename",
                None,
                Shortcut::parse("F2"),
                false,
                false,
                UnsyncCallback::new(|()| {}),
            );
            assert!(!has_leading_column(&plain.entries));

            let mut mixed = popup_in(MenuModel::new());
            mixed.item(
                "New tab",
                Some(IconName::Plus),
                Shortcut::parse("Ctrl+N"),
                false,
                false,
                UnsyncCallback::new(|()| {}),
            );
            mixed.push(MenuEntry::Separator);
            mixed.item(
                "Paste",
                None,
                Shortcut::parse("Ctrl+V"),
                false,
                false,
                UnsyncCallback::new(|()| {}),
            );
            assert!(has_leading_column(&mixed.entries));
            let rendered = render_entries(mixed.entries);
            assert_eq!(rendered.len(), 3);
        });
        scope.unmount();
    }

    #[test]
    fn context_menu_expands_beyond_compact_dropdown_cap_at_large_font_sizes() {
        let popup_open_slot: Rc<RefCell<Option<RwSignal<bool, LocalStorage>>>> =
            Rc::new(RefCell::new(None));
        let slot_view = Rc::clone(&popup_open_slot);

        let driver = move |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
            let mut harness = Harness::new(handler);
            let offscreen = harness
                .platform()
                .offscreens()
                .first()
                .cloned()
                .expect("offscreen surface");
            offscreen.set_a11y_listener(true);
            harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(1024.0),
                DevicePx(800.0),
            )));
            harness.settle(12);

            // 1. Right-click at (120, 48) to open a 30-item ContextMenuHost (~920px uncapped at 16px font):
            // must expand well past 360px/440px AND cap cleanly to Popper/window available height (<= 782px).
            let pos = Point::new(CssPx(120.0), CssPx(48.0));
            harness.deliver_to_first(SurfaceEvent::Pointer {
                action: PointerAction::Pressed,
                event: PointerEvent::mouse(pos).with_button(PointerButton::Secondary),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
            harness.deliver_to_first(SurfaceEvent::Pointer {
                action: PointerAction::Released,
                event: PointerEvent::mouse(pos).with_button(PointerButton::Secondary),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
            harness.settle(16);

            let ctx_heights = live_menu_heights(&offscreen);
            assert!(
                ctx_heights
                    .iter()
                    .any(|h| *h > 700.0 && *h <= 800.0 - 18.0 + 0.5),
                "expected 30-item ContextMenuHost bounded by available height (> 700px and <= 782px), got {ctx_heights:?}"
            );

            // Dismiss context menu and open 20-item compact PopupMenu: must stay capped at <= 360px.
            let dismiss_pos = Point::new(CssPx(900.0), CssPx(700.0));
            harness.deliver_to_first(SurfaceEvent::Pointer {
                action: PointerAction::Pressed,
                event: PointerEvent::mouse(dismiss_pos).with_button(PointerButton::Primary),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
            harness.deliver_to_first(SurfaceEvent::Pointer {
                action: PointerAction::Released,
                event: PointerEvent::mouse(dismiss_pos).with_button(PointerButton::Primary),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
            harness.settle(12);

            let popup_open = popup_open_slot.borrow().expect("popup signal");
            popup_open.set(true);
            flush();
            harness.settle(16);

            let popup_heights = live_menu_heights(&offscreen);
            assert!(
                popup_heights.iter().any(|h| (*h - 360.0).abs() <= 1.5),
                "expected 20-item compact PopupMenu capped at 360px, got {popup_heights:?}"
            );

            harness.shut_down();
            Ok(())
        };

        zgui::app()
            .with_fonts(Fonts::shipped_only())
            .with_renderer(Box::new(noop_renderer))
            .with_size(1024.0, 800.0)
            .run_on(driver, move || {
                let app = Gds::install(Scope::default());
                app.appearance().motion().set_enabled(false);
                let typo = crate::appearance::typography::Preferences {
                    size: 16.0,
                    ..Default::default()
                };
                app.appearance()
                    .typography()
                    .apply(&typo)
                    .expect("16px typography");
                let _ = app.appearance().install_window_sheet();

                let popup_open = RwSignal::new_local(false);
                *slot_view.borrow_mut() = Some(popup_open);

                let build_ctx = Rc::new(|| {
                    let mut m = MenuModel::new();
                    for idx in 1..=30 {
                        m.item(
                            format!("Context action {idx}"),
                            Some(IconName::Folder),
                            Shortcut::parse("Ctrl+N"),
                            false,
                            false,
                            UnsyncCallback::new(|()| {}),
                        );
                        if idx == 5 || idx == 11 {
                            m.separator();
                        }
                    }
                    m
                });

                let mut popup_model = MenuModel::new();
                for idx in 1..=20 {
                    popup_model.item(
                        format!("Dropdown option {idx}"),
                        Some(IconName::Folder),
                        Shortcut::parse("Ctrl+N"),
                        false,
                        false,
                        UnsyncCallback::new(|()| {}),
                    );
                }
                let popup_sig = Signal::stored_local(popup_model);

                view! {
                    column(style:width = "100%", style:height = "100%") {
                        row(style:height = "40px") {
                            PopupMenu(
                                open = popup_open,
                                model = popup_sig,
                                trigger = || AnyView::new(view! { text { "Open dropdown" } })
                            )
                        }
                        ContextMenuHost(model = build_ctx) {
                            box(style:width = "600px", style:height = "500px") {
                                text { "Target area" }
                            }
                        }
                    }
                }
            })
            .expect("menu height harness");
    }
}
