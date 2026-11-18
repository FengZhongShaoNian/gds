#![allow(dead_code)]
//! Page-specific table metadata uses each domain's own values and service state.

mod network;

use std::path::PathBuf;

use gds::service::remote;
use gds::ui::button::{ButtonSize, ButtonVariant};
use gds::ui::capacity::CapacityBarProps;
use gds::ui::feedback::tooltip::HintTooltipProps;
use gds::ui::page::PageNoticeProps;
use gds::ui::time::Timestamp;
use gds::ui::{IconButtonProps, IconName, IconProps};
use gds::{i18n, t};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, Signal};
use zgui::view::{current_host, install_stylesheet};
use zgui::vocab::Modifiers;
use zgui::{css, view};
use zgui_ui::{
    ButtonProps, DropdownMenuContentProps, DropdownMenuProps, DropdownMenuTriggerProps,
    MenuCheckboxItemProps, SpinnerProps,
};
use zgui_ui_primitives::Binding;

use crate::entities::file::{
    columns::Column,
    sorting::Direction,
    ui::{EntryPresentation, UNKNOWN_VALUE},
};
use crate::entities::location::{Location, volumes};

use super::{
    model::{Contents, Row},
    ui::Browser,
};

const SURFACES_SHEET: &str = "gds-files-workspace-surfaces";

const SURFACES_CSS: &str = css!(
    r#"
    .gds-files-collection-state {
        flex: 1 1 0%;
        min-height: 220px;
        align-items: center;
        justify-content: center;
        gap: 10px;
        padding: 24px;
    }

    .gds-files-collection-state__icon {
        width: 32px;
        height: 32px;
        color: var(--gds-muted-foreground);
    }

    .gds-files-collection-state__title {
        font-size: var(--gds-font-size-md, 16px);
        font-weight: 500;
        color: var(--gds-foreground);
        text-align: center;
    }

    .gds-files-collection-state__detail {
        font-size: var(--gds-font-size-sm, 14px);
        color: var(--gds-muted-foreground);
        text-align: center;
    }

    .gds-files-collection-notice {
        align-items: center;
        justify-content: space-between;
        gap: 10px;
    }

    .gds-files-collection-notice--error {
        border-color: var(--gds-error);
    }

    .gds-files-collection-notice__lead {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 8px;
        min-width: 0;
        flex: 1 1 0%;
    }

    .gds-files-collection-notice__text {
        font-size: var(--gds-font-size-sm, 14px);
        color: var(--gds-foreground);
    }

    .gds-files-collection-header {
        display: flex;
        flex-direction: row;
        align-items: center;
        height: 32px;
        padding: 0 8px;
        border-bottom: 1px solid var(--gds-border);
        background: var(--gds-surface);
    }

    .gds-files-collection-header__cell {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 4px;
        min-width: 0;
        flex-shrink: 0;
        padding: 0 6px;
        font-size: var(--gds-font-size-xs, 12px);
        font-weight: 600;
        color: var(--gds-muted-foreground);
    }

    .gds-files-collection-header__button {
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: flex-start;
        gap: 4px;
        width: 100%;
        height: 26px;
        padding: 0 4px;
        font-size: var(--gds-font-size-xs, 12px);
        font-weight: 600;
        color: var(--gds-muted-foreground);
    }

    .gds-files-collection-header__button[data-active="true"] {
        color: var(--gds-foreground);
    }

    .gds-files-collection-cell {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 8px;
        min-width: 0;
        flex-shrink: 0;
        padding: 0 6px;
        overflow: hidden;
    }

    .gds-files-collection-cell--split {
        justify-content: space-between;
    }

    .gds-files-collection-cell__text {
        font-size: var(--gds-font-size-sm, 14px);
        color: var(--gds-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        min-width: 0;
        flex: 1 1 0%;
    }

    .gds-files-collection-cell__text[data-muted="true"] {
        color: var(--gds-muted-foreground);
    }

    .gds-files-collection-cell__badge {
        font-size: var(--gds-font-size-xs, 12px);
        color: var(--gds-muted-foreground);
        flex-shrink: 0;
    }

    .gds-files-collection-mount-picker {
        max-width: 100%;
        justify-content: space-between;
        gap: 4px;
        padding: 2px 6px;
        height: 24px;
    }

    .gds-files-collection-mount-picker__label {
        font-size: var(--gds-font-size-xs, 12px);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        min-width: 0;
    }
    "#
);

fn install_surfaces_stylesheet() {
    if current_host().is_some() {
        install_stylesheet(SURFACES_SHEET, SURFACES_CSS);
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct CollectionBodyState {
    pub icon: Option<IconName>,
    pub title: String,
    pub detail: Option<String>,
    pub retry: bool,
    pub loading: bool,
    pub reconnect: bool,
}

impl CollectionBodyState {
    pub(super) fn view(self, browser: &Browser) -> AnyView {
        install_surfaces_stylesheet();
        let retry_browser = browser.clone();
        let reconnect_browser = browser.clone();
        let title_for_spinner = self.title.clone();
        let status_mark = if self.loading {
            Some(AnyView::new(view! { Spinner(label = title_for_spinner) }))
        } else {
            self.icon.map(|icon| {
                AnyView::new(view! {
                    Icon(name = icon, class = "gds-files-collection-state__icon")
                })
            })
        };

        let detail_view = self.detail.map(|detail| {
            view! {
                text(class = "gds-files-collection-state__detail") {{detail}}
            }
        });

        let action_view = if self.reconnect {
            Some(AnyView::new(view! {
                Button(
                    variant = ButtonVariant::Outline,
                    on:click = move |_| reconnect_browser.reconnect_volume(),
                ) {{i18n::text("device-reconnect")}}
            }))
        } else if self.retry {
            Some(AnyView::new(view! {
                Button(
                    variant = ButtonVariant::Outline,
                    on:click = move |_| retry_browser.refresh(),
                ) {{i18n::text("retry")}}
            }))
        } else {
            None
        };

        AnyView::new(view! {
            column(class = "gds-files-collection-state") {
                {status_mark}
                text(class = "gds-files-collection-state__title") {{self.title}}
                {detail_view}
                {action_view}
            }
        })
    }
}

fn plain_text_cell(
    text: String,
    muted: bool,
    tooltip: Option<String>,
    width: Option<f32>,
) -> AnyView {
    let label = view! {
        text(
            class = "gds-files-collection-cell__text",
            attr:data-muted = muted.then(|| "true".to_owned()),
        ) {{text}}
    };
    let inner = match tooltip {
        Some(tip) => AnyView::new(view! {
            HintTooltip(text = tip) {
                {label}
            }
        }),
        None => AnyView::new(label),
    };
    match width {
        Some(w) => AnyView::new(view! {
            row(class = "gds-files-collection-cell", style:width = format!("{w:.1}px")) {
                {inner}
            }
        }),
        None => AnyView::new(view! {
            row(class = "gds-files-collection-cell") {
                {inner}
            }
        }),
    }
}

fn remote_entry_icon(kind: remote::EntryKind) -> IconName {
    match kind {
        remote::EntryKind::Directory => IconName::Folder,
        remote::EntryKind::SymbolicLink => IconName::Link,
        remote::EntryKind::File => IconName::File,
        remote::EntryKind::Other => IconName::FileQuestionMark,
    }
}

fn active_mount_point(browser: &Browser, volume: &volumes::Volume) -> Option<PathBuf> {
    let preferred = volume.id.stable.as_ref().and_then(|id| {
        browser
            .0
            .preferences
            .snapshot()
            .files
            .volume_mount_points
            .get(id)
            .cloned()
    });
    preferred
        .filter(|candidate| volume.mount_points.contains(candidate))
        .or_else(|| {
            volume
                .mount_points
                .iter()
                .min_by(|left, right| left.components().count().cmp(&right.components().count()))
                .cloned()
        })
}

fn volume_capacity_cell(
    browser: &Browser,
    volume: &volumes::Volume,
    selected: Signal<bool, LocalStorage>,
    width: f32,
) -> AnyView {
    let state = browser.0.volumes.capacity(&volume.id);
    let (label, fraction, unavailable) = match state {
        None | Some(volumes::CapacityState::Loading) => {
            if volume.mount_points.is_empty() {
                (i18n::text("device-not-mounted"), None, true)
            } else {
                (i18n::text("device-capacity-loading"), None, false)
            }
        }
        Some(volumes::CapacityState::Ready(capacity)) => {
            let available = EntryPresentation::format_size(capacity.available);
            let total = EntryPresentation::format_size(capacity.total);
            (
                t!("device-capacity", available, total),
                Some(capacity.unavailable_fraction()),
                false,
            )
        }
        Some(volumes::CapacityState::Unavailable(reason)) => {
            let message = if volume.mount_points.is_empty() {
                i18n::text("device-not-mounted")
            } else {
                reason
            };
            (message, None, true)
        }
    };

    AnyView::new(view! {
        row(class = "gds-files-collection-cell", style:width = format!("{width:.1}px")) {
            CapacityBar(
                label = label,
                fraction = fraction,
                unavailable = unavailable,
                selected = selected,
            )
        }
    })
}

fn volume_mount_point_cell(browser: &Browser, volume: &volumes::Volume, width: f32) -> AnyView {
    let busy = browser.0.volumes.is_busy(&volume.id);
    let action_button = if volume.mountable && volume.mount_points.is_empty() {
        let b = browser.clone();
        let id = volume.id.clone();
        Some(AnyView::new(view! {
            IconButton(
                icon = IconName::Plug,
                label = i18n::text("device-mount"),
                disabled = busy,
                on:pointer_down = move |ev| ev.stop_propagation(),
                on:click = move |ev| {
                    ev.stop_propagation();
                    b.change_volume(volumes::DeviceAction::Mount(id.clone()));
                },
            )
        }))
    } else if volume.can_unmount() {
        let b = browser.clone();
        let id = volume.id.clone();
        Some(AnyView::new(view! {
            IconButton(
                icon = IconName::Unplug,
                label = i18n::text("device-unmount"),
                disabled = busy,
                on:pointer_down = move |ev| ev.stop_propagation(),
                on:click = move |ev| {
                    ev.stop_propagation();
                    b.change_volume(volumes::DeviceAction::Unmount(id.clone()));
                },
            )
        }))
    } else {
        None
    };

    let lead_view = if volume.mount_points.is_empty() {
        let text = if !volume.mountable && volume.filesystem.is_empty() {
            i18n::text("device-not-browsable")
        } else {
            i18n::text("device-not-mounted")
        };
        AnyView::new(view! {
            text(class = "gds-files-collection-cell__text", attr:data-muted = "true") {{text}}
        })
    } else if volume.mount_points.len() == 1 {
        let path_text = volume.mount_points[0].display().to_string();
        let tooltip = path_text.clone();
        AnyView::new(view! {
            HintTooltip(text = tooltip) {
                text(class = "gds-files-collection-cell__text") {{path_text}}
            }
        })
    } else {
        let current = active_mount_point(browser, volume);
        let current_label = current
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| UNKNOWN_VALUE.to_owned());
        let stable_id = volume.id.stable.clone();
        let can_select = stable_id.is_some();

        let mount_points = volume.mount_points.clone();
        let menu_browser = browser.clone();

        let trigger = AnyView::new(view! {
            DropdownMenu {
                DropdownMenuTrigger(
                    variant = ButtonVariant::Outline,
                    size = ButtonSize::Sm,
                    class = "gds-files-collection-mount-picker",
                    on:pointer_down = move |ev| ev.stop_propagation(),
                    on:click = move |ev| ev.stop_propagation(),
                ) {
                    row(style:gap = "4px", style:min-width = "0") {
                        text(class = "gds-files-collection-mount-picker__label") {{current_label}}
                        Icon(name = IconName::ChevronDown) {}
                    }
                }
                DropdownMenuContent(class = "gds-menu-content") {
                    {mount_points
                        .iter()
                        .map(|mount| {
                            let is_current = current.as_ref() == Some(mount);
                            let label = mount.display().to_string();
                            let target_mount = mount.clone();
                            let b = menu_browser.clone();
                            let stable = stable_id.clone();
                            AnyView::new(view! {
                                MenuCheckboxItem(
                                    default_checked = is_current,
                                    disabled = !can_select,
                                    on_change = UnsyncCallback::new(move |_| {
                                        if let Some(id) = stable.clone() {
                                            let chosen = target_mount.clone();
                                            b.edit_preferences(|prefs| {
                                                prefs.files.volume_mount_points.insert(id, chosen);
                                            });
                                            b.changed();
                                        }
                                    }),
                                ) {
                                    {label}
                                }
                            })
                        })
                        .collect::<Vec<_>>()}
                }
            }
        });

        if can_select {
            trigger
        } else {
            AnyView::new(view! {
                HintTooltip(text = i18n::text("volume-default-unavailable")) {
                    {trigger}
                }
            })
        }
    };

    AnyView::new(view! {
        row(
            class = "gds-files-collection-cell gds-files-collection-cell--split",
            style:width = format!("{width:.1}px"),
        ) {
            {lead_view}
            {action_button}
        }
    })
}

impl Browser {
    pub(super) fn collection_notice(&self) -> Option<AnyView> {
        install_surfaces_stylesheet();
        let (location, contents_kind, has_rows) = self.read(|state| {
            let tab = state.tab();
            let kind = match &tab.contents {
                Contents::Tags { .. } => 1_u8,
                Contents::Volumes => 2_u8,
                _ => 0_u8,
            };
            (
                tab.navigation.location().clone(),
                kind,
                !tab.rows.is_empty(),
            )
        });

        if matches!(location, Location::Tags(_))
            && !self.0.preferences.snapshot().tags.enabled
            && contents_kind == 1
        {
            let b = self.clone();
            return Some(AnyView::new(view! {
                PageNotice(class = "gds-files-collection-notice") {
                    row(class = "gds-files-collection-notice__lead") {
                        Icon(name = IconName::Tag)
                        text(class = "gds-files-collection-notice__text") {{i18n::text("tags-disabled")}}
                    }
                    Button(
                        variant = ButtonVariant::Outline,
                        on:click = move |_| b.show_settings(),
                    ) {{i18n::text("settings")}}
                }
            }));
        }

        if contents_kind == 2
            && has_rows
            && let Some(error) = self.0.volumes.error()
        {
            let b = self.clone();
            return Some(AnyView::new(view! {
                PageNotice(class = "gds-files-collection-notice gds-files-collection-notice--error") {
                    row(class = "gds-files-collection-notice__lead") {
                        Icon(name = IconName::CircleAlert)
                        text(class = "gds-files-collection-notice__text") {{error}}
                    }
                    Button(
                        variant = ButtonVariant::Outline,
                        on:click = move |_| b.refresh(),
                    ) {{i18n::text("retry")}}
                }
            }));
        }

        None
    }

    pub(super) fn collection_body_state(&self) -> Option<CollectionBodyState> {
        let (location, is_loading, unavailable, is_volumes, empty_rows) = self.read(|state| {
            let tab = state.tab();
            let location = tab.navigation.location().clone();
            let (is_loading, unavailable, is_volumes) = match &tab.contents {
                Contents::LoadingCollection => (true, None, false),
                Contents::Unavailable(msg) => (false, Some(msg.clone()), false),
                Contents::Volumes => (false, None, true),
                _ => (false, None, false),
            };
            (
                location,
                is_loading,
                unavailable,
                is_volumes,
                tab.rows.is_empty(),
            )
        });

        if is_loading {
            let title = match &location {
                Location::Trash => i18n::text("recycle-loading"),
                Location::Network => i18n::text("network-loading"),
                Location::Volumes => i18n::text("device-discovering"),
                Location::Tags(_) => i18n::text("tags-loading"),
                _ => i18n::text("loading"),
            };
            return Some(CollectionBodyState {
                icon: None,
                title,
                detail: None,
                retry: false,
                loading: true,
                reconnect: false,
            });
        }

        if let Some(error) = unavailable {
            return Some(match &location {
                Location::Trash => CollectionBodyState {
                    icon: Some(IconName::Trash),
                    title: i18n::text("recycle-load-failed"),
                    detail: Some(error),
                    retry: true,
                    loading: false,
                    reconnect: false,
                },
                Location::Tags(_) => {
                    let enabled = self.0.preferences.snapshot().tags.enabled;
                    CollectionBodyState {
                        icon: Some(IconName::Tag),
                        title: if enabled {
                            i18n::text("tags-unavailable")
                        } else {
                            i18n::text("tags-disabled")
                        },
                        detail: enabled.then_some(error),
                        retry: enabled,
                        loading: false,
                        reconnect: false,
                    }
                }
                Location::Volumes => CollectionBodyState {
                    icon: Some(IconName::HardDrive),
                    title: i18n::text("device-operation-failed"),
                    detail: Some(error),
                    retry: true,
                    loading: false,
                    reconnect: false,
                },
                Location::Volume { identity, .. } => {
                    let attached = self.volume_at_location(identity).is_some();
                    CollectionBodyState {
                        icon: Some(IconName::HardDrive),
                        title: if attached {
                            error.clone()
                        } else {
                            i18n::text("device-disconnected")
                        },
                        detail: (!attached).then_some(error),
                        retry: attached,
                        loading: false,
                        reconnect: !attached,
                    }
                }
                _ => CollectionBodyState {
                    icon: Some(IconName::CircleAlert),
                    title: error,
                    detail: None,
                    retry: true,
                    loading: false,
                    reconnect: false,
                },
            });
        }

        if is_volumes && empty_rows {
            if self.0.volumes.discovering() {
                return Some(CollectionBodyState {
                    icon: None,
                    title: i18n::text("device-discovering"),
                    detail: None,
                    retry: false,
                    loading: true,
                    reconnect: false,
                });
            }
            if let Some(error) = self.0.volumes.error() {
                return Some(CollectionBodyState {
                    icon: Some(IconName::HardDrive),
                    title: i18n::text("device-operation-failed"),
                    detail: Some(error),
                    retry: true,
                    loading: false,
                    reconnect: false,
                });
            }
        }

        None
    }

    pub(super) fn collection_empty_label(&self) -> Option<&'static str> {
        self.read(|state| match &state.tab().contents {
            Contents::Places(_) | Contents::Tags { .. } => Some("empty-collection"),
            Contents::Trash(_) => Some("recycle-empty"),
            Contents::Remote { .. } => Some("empty-directory"),
            Contents::Volumes => Some("device-none"),
            Contents::Network { .. } => Some("network-empty"),
            Contents::Directory | Contents::LoadingCollection | Contents::Unavailable(_) => None,
        })
    }

    pub(super) fn collection_columns(&self) -> Option<Vec<(&'static str, f32)>> {
        let viewport = self.table_viewport_width().max(240.0);
        let compact = self.compact();

        self.read(|state| {
            let tab = state.tab();
            match &tab.contents {
                Contents::Places(_) => {
                    let loc_w = if compact { 200.0 } else { 280.0 };
                    let name_w = (viewport - loc_w).max(180.0);
                    Some(vec![("name", name_w), ("place-location", loc_w)])
                }
                Contents::Trash(_) => {
                    let orig_w = if compact { 170.0 } else { 240.0 };
                    let deleted_w = if compact { 140.0 } else { 170.0 };
                    let size_w = 96.0;
                    let name_w = (viewport - orig_w - deleted_w - size_w).max(180.0);
                    Some(vec![
                        ("name", name_w),
                        ("recycle-original-location", orig_w),
                        ("recycle-deleted-at", deleted_w),
                        ("size", size_w),
                    ])
                }
                Contents::Tags { .. } => {
                    // Pure tag catalogues never show file metadata columns (testing.md).
                    let has_files = tab.rows.iter().any(|row| matches!(row, Row::File(_)));
                    if has_files {
                        None
                    } else {
                        Some(vec![("name", viewport)])
                    }
                }
                Contents::Remote { .. } => {
                    let modified_w = if compact { 140.0 } else { 170.0 };
                    let size_w = 96.0;
                    let name_w = (viewport - modified_w - size_w).max(180.0);
                    Some(vec![
                        ("name", name_w),
                        ("modified", modified_w),
                        ("size", size_w),
                    ])
                }
                Contents::Volumes => {
                    let format_w = if compact { 90.0 } else { 110.0 };
                    let space_w = if compact { 170.0 } else { 220.0 };
                    let mount_w = if compact { 160.0 } else { 210.0 };
                    let name_w = (viewport - format_w - space_w - mount_w).max(180.0);
                    Some(vec![
                        ("name", name_w),
                        ("volume-format", format_w),
                        ("volume-space", space_w),
                        ("volume-default-mount", mount_w),
                    ])
                }
                Contents::Directory
                | Contents::LoadingCollection
                | Contents::Unavailable(_)
                | Contents::Network { .. } => None,
            }
        })
    }

    pub(super) fn collection_header(&self) -> Option<AnyView> {
        install_surfaces_stylesheet();
        let columns = self.collection_columns()?;
        let sortable = self.sortable_columns();
        let sort = self.read(|state| state.tab().sort);

        let cells: Vec<AnyView> = columns
            .into_iter()
            .map(|(label_id, width)| {
                let column = Column::from_id(label_id).filter(|col| sortable.contains(col));
                if let Some(col) = column {
                    let active = sort.key == col;
                    let b = self.clone();
                    let indicator = active.then(|| {
                        let icon = match sort.direction {
                            Direction::Ascending => IconName::ArrowUp,
                            Direction::Descending => IconName::ArrowDown,
                        };
                        view! { Icon(name = icon) }.into_view()
                    });
                    AnyView::new(view! {
                        row(class = "gds-files-collection-header__cell", style:width = format!("{width:.1}px")) {
                            Button(
                                variant = ButtonVariant::Ghost,
                                size = ButtonSize::Sm,
                                class = "gds-files-collection-header__button",
                                attr:data-active = active.then(|| "true".to_owned()),
                                on:click = move |_| b.column_sort(col),
                            ) {
                                text {{i18n::text(label_id)}}
                                {indicator}
                            }
                        }
                    })
                } else {
                    AnyView::new(view! {
                        row(class = "gds-files-collection-header__cell", style:width = format!("{width:.1}px")) {
                            text {{i18n::text(label_id)}}
                        }
                    })
                }
            })
            .collect();

        Some(AnyView::new(view! {
            row(class = "gds-files-collection-header") {
                {cells}
            }
        }))
    }

    pub(super) fn collection_cells(
        &self,
        row: &Row,
        selected: Binding<bool>,
    ) -> Option<Vec<AnyView>> {
        install_surfaces_stylesheet();
        let columns = self.collection_columns();
        let col_widths: Vec<f32> = columns
            .as_ref()
            .map(|cols| cols.iter().map(|(_, w)| *w).collect())
            .unwrap_or_default();
        let width_at = |idx: usize| col_widths.get(idx).copied();

        let sel_f32 = Signal::derive_local(move || {
            if selected.get().unwrap_or(false) {
                1.0
            } else {
                0.0
            }
        });
        let sel_bool = Signal::derive_local(move || selected.get().unwrap_or(false));

        match row {
            Row::File(_) => None,
            Row::Place(location) => {
                let artwork = self.row_artwork_with_selection(row, 18.0, sel_f32);
                let label = location.label();
                let address = location.display_address();
                let name_w = width_at(0);
                let name_cell = match name_w {
                    Some(w) => AnyView::new(view! {
                        row(class = "gds-files-collection-cell", style:width = format!("{w:.1}px")) {
                            {artwork}
                            text(class = "gds-files-collection-cell__text") {{label}}
                        }
                    }),
                    None => AnyView::new(view! {
                        row(class = "gds-files-collection-cell") {
                            {artwork}
                            text(class = "gds-files-collection-cell__text") {{label}}
                        }
                    }),
                };
                Some(vec![
                    name_cell,
                    plain_text_cell(address.clone(), true, Some(address), width_at(1)),
                ])
            }
            Row::Trash(entry) => {
                let artwork = self.row_artwork_with_selection(row, 18.0, sel_f32);
                let label = entry.name.to_string_lossy().into_owned();
                let key = row.key();
                let b = self.clone();
                let restore_button = AnyView::new(view! {
                    IconButton(
                        icon = IconName::ArchiveRestore,
                        label = i18n::text("recycle-restore"),
                        on:pointer_down = move |ev| ev.stop_propagation(),
                        on:click = move |ev| {
                            ev.stop_propagation();
                            let found = {
                                let state = b.0.state.borrow();
                                state.tab().rows.iter().position(|item| item.key() == key)
                            };
                            if let Some(index) = found {
                                b.select_row(index, Modifiers::default());
                                b.restore_trash(false, false);
                            }
                        },
                    )
                });
                let name_w = width_at(0);
                let name_cell = match name_w {
                    Some(w) => AnyView::new(view! {
                        row(
                            class = "gds-files-collection-cell gds-files-collection-cell--split",
                            style:width = format!("{w:.1}px"),
                        ) {
                            row(style:gap = "8px", style:min-width = "0", style:flex = "1 1 0%", style:align-items = "center") {
                                {artwork}
                                text(class = "gds-files-collection-cell__text") {{label}}
                            }
                            {restore_button}
                        }
                    }),
                    None => AnyView::new(view! {
                        row(class = "gds-files-collection-cell gds-files-collection-cell--split") {
                            row(style:gap = "8px", style:min-width = "0", style:flex = "1 1 0%", style:align-items = "center") {
                                {artwork}
                                text(class = "gds-files-collection-cell__text") {{label}}
                            }
                            {restore_button}
                        }
                    }),
                };

                let original = entry
                    .original_path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or(&entry.original_path)
                    .display()
                    .to_string();
                let full_original = entry.original_path.display().to_string();

                let deleted_at = entry.deleted_at;
                let deleted_view = view! { {Timestamp::new(deleted_at)} };
                let deleted_cell = match width_at(2) {
                    Some(w) => AnyView::new(view! {
                        row(class = "gds-files-collection-cell", style:width = format!("{w:.1}px")) {
                            {deleted_view}
                        }
                    }),
                    None => AnyView::new(view! {
                        row(class = "gds-files-collection-cell") {
                            {deleted_view}
                        }
                    }),
                };

                let (size_label, muted) = match entry.size {
                    Some(bytes) => (EntryPresentation::format_size(bytes), false),
                    None => match entry.items {
                        Some(items) => (items.to_string(), false),
                        None => (UNKNOWN_VALUE.to_owned(), true),
                    },
                };

                Some(vec![
                    name_cell,
                    plain_text_cell(original, true, Some(full_original), width_at(1)),
                    deleted_cell,
                    plain_text_cell(size_label, muted, None, width_at(3)),
                ])
            }
            Row::Tag(tag) => {
                let artwork = self.row_artwork_with_selection(row, 18.0, sel_f32);
                let leaf = row.label();
                let tip = tag.contains('/').then(|| tag.clone());
                let name_inner = match tip {
                    Some(full_tag) => AnyView::new(view! {
                        HintTooltip(text = full_tag) {
                            text(class = "gds-files-collection-cell__text") {{leaf}}
                        }
                    }),
                    None => AnyView::new(view! {
                        text(class = "gds-files-collection-cell__text") {{leaf}}
                    }),
                };
                let name_cell = match width_at(0) {
                    Some(w) => AnyView::new(view! {
                        row(class = "gds-files-collection-cell", style:width = format!("{w:.1}px")) {
                            {artwork}
                            {name_inner}
                        }
                    }),
                    None => AnyView::new(view! {
                        row(class = "gds-files-collection-cell") {
                            {artwork}
                            {name_inner}
                        }
                    }),
                };

                if columns.is_some() {
                    Some(vec![name_cell])
                } else {
                    // Mixed tag view (both subtags and files): pad extra directory columns.
                    let extra_count =
                        self.read(|state| state.tab().columns.columns().len().saturating_sub(1));
                    let mut cells = Vec::with_capacity(1 + extra_count);
                    cells.push(name_cell);
                    for _ in 0..extra_count {
                        cells.push(plain_text_cell(UNKNOWN_VALUE.to_owned(), true, None, None));
                    }
                    Some(cells)
                }
            }
            Row::Remote(entry) => {
                let icon = remote_entry_icon(entry.kind);
                let name = entry.name.clone();
                let name_cell = match width_at(0) {
                    Some(w) => AnyView::new(view! {
                        row(class = "gds-files-collection-cell", style:width = format!("{w:.1}px")) {
                            Icon(name = icon)
                            text(class = "gds-files-collection-cell__text") {{name}}
                        }
                    }),
                    None => AnyView::new(view! {
                        row(class = "gds-files-collection-cell") {
                            Icon(name = icon)
                            text(class = "gds-files-collection-cell__text") {{name}}
                        }
                    }),
                };

                let modified_view = view! { {Timestamp::new(entry.modified)} };
                let modified_cell = match width_at(1) {
                    Some(w) => AnyView::new(view! {
                        row(class = "gds-files-collection-cell", style:width = format!("{w:.1}px")) {
                            {modified_view}
                        }
                    }),
                    None => AnyView::new(view! {
                        row(class = "gds-files-collection-cell") {
                            {modified_view}
                        }
                    }),
                };

                let (size_text, size_muted) = match entry.size {
                    Some(bytes) => (EntryPresentation::format_size(bytes), false),
                    None => (UNKNOWN_VALUE.to_owned(), true),
                };

                Some(vec![
                    name_cell,
                    modified_cell,
                    plain_text_cell(size_text, size_muted, None, width_at(2)),
                ])
            }
            Row::Volume(volume) => {
                let artwork = self.row_artwork_with_selection(row, 18.0, sel_f32);
                let label = volume.name.clone();
                let lock_icon = volume
                    .locked
                    .then(|| view! { Icon(name = IconName::Lock) }.into_view());
                let system_badge = volume.system_mount.then(|| {
                    view! {
                        text(class = "gds-files-collection-cell__badge") {{i18n::text("device-system-volume")}}
                    }
                    .into_view()
                });
                let name_cell = match width_at(0) {
                    Some(w) => AnyView::new(view! {
                        row(class = "gds-files-collection-cell", style:width = format!("{w:.1}px")) {
                            {artwork}
                            text(class = "gds-files-collection-cell__text") {{label}}
                            {lock_icon}
                            {system_badge}
                        }
                    }),
                    None => AnyView::new(view! {
                        row(class = "gds-files-collection-cell") {
                            {artwork}
                            text(class = "gds-files-collection-cell__text") {{label}}
                            {lock_icon}
                            {system_badge}
                        }
                    }),
                };

                let (format_text, format_muted) = if volume.filesystem.is_empty() {
                    (UNKNOWN_VALUE.to_owned(), true)
                } else {
                    (volume.filesystem.clone(), false)
                };

                let space_w = width_at(2).unwrap_or(200.0);
                let mount_w = width_at(3).unwrap_or(180.0);

                Some(vec![
                    name_cell,
                    plain_text_cell(format_text, format_muted, None, width_at(1)),
                    volume_capacity_cell(self, volume, sel_bool, space_w),
                    volume_mount_point_cell(self, volume, mount_w),
                ])
            }
        }
    }
}
