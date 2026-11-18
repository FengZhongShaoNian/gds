use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
};

use gds::app::Gds;
use gds::appearance::{
    motion::{WORKSPACE_DURATION_MS, ease_out_cubic, use_transition},
    tokens::DataListStyle,
};
use gds::i18n;
use gds::ui::about::{AboutProps, Application, HEIGHT as ABOUT_HEIGHT, WIDTH as ABOUT_WIDTH};
use gds::ui::button::IconButtonProps;
use gds::ui::feedback::dialog::{
    DialogActionsProps, DialogContentProps, DialogDecision, DialogDescriptionProps,
    DialogFooterProps, DialogHeaderProps, DialogProps, DialogScope, DialogTitleProps,
    handle_dialog_key_down,
};
use gds::ui::feedback::notifications::{NotificationHostProps, error as notify_error};
use gds::ui::feedback::tooltip::{HintTooltipProps, wrap_tokens};
use gds::ui::frame::{
    FooterProps, FrameProps, ToolbarProps, WindowDragAreaProps, WorkspaceFrameProps,
};
use gds::ui::interaction::{
    keytips::KeytipHostProps,
    region::{is_focus_visible, surface_with_shortcut, use_keyboard_modality},
};
use gds::ui::list::ListSurfaceProps;
use gds::ui::page::{
    ContentPageProps, PageActionsProps, PageContentProps, PageHeaderProps, PageTableAreaProps,
    PageTitleProps, TABLE_INSET,
};
use gds::ui::time::TimestampViewProps;
use gds::ui::window::WindowControlsProps;
use gds::ui::{IconName, IconProps};
use zgui::geom::{CssPx, Point};
use zgui::prelude::*;
use zgui::reactive::{
    Owner, RenderEffect, RwSignal, Signal, UnsyncCallback, enter_non_reactive_zone,
    on_cleanup_local,
};
use zgui::view::time::{FrameHandle, Timers};
use zgui::view::{Scoped, current_host, events, focus_is_text_entry, install_stylesheet};
use zgui::vocab::{PointerButton, Role, SharedString, Timestamp};
use zgui::{css, view};
use zgui_ui::{ButtonProps, ButtonVariant, SpinnerProps, VirtualListProps, Virtualize};
use zgui_ui_primitives::Binding;

use super::geometry::{GRID_GAP, GRID_PADDING, GridMotion};
use super::marquee;
use super::menus::Surface;
use super::model::{ConfirmOptions, Contents, Pane, Prompt, Row};
use super::name_form::PromptDecisionHandler;
use super::ui::Browser;
use crate::entities::file::ViewMode;
use crate::entities::file::columns::Column;
use crate::entities::file::sorting::Direction;
use crate::entities::file::ui::EntryPresentation;
use crate::entities::location::Location;
use crate::shared::lib::clipboard::Clipboard;
use crate::shared::ui::actions::Command;
use crate::widgets::help::FilesHelpProps;

const ROW_HEIGHT: f32 = DataListStyle::ROW_HEIGHT;
const HEADER_HEIGHT: f32 = DataListStyle::HEADER_HEIGHT;
const DETAILS_WIDTH: f32 = 280.0;
const TAB_ENTER_INITIAL_OPACITY: f32 = 0.88;
const CONFIRM_PREVIEW_LIMIT: usize = 5;
const WORKSPACE_SHEET: &str = "gds-files-workspace";

const WORKSPACE_CSS: &str = css!(
    r#"
    .gds-files-root {
        display: flex;
        flex-direction: column;
        width: 100%;
        height: 100%;
        min-width: 0px;
        min-height: 0px;
    }

    .gds-files-root:focus {
        outline: none;
    }

    .gds-files-frame {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        height: 100%;
        min-width: 0px;
        min-height: 0px;
    }

    .gds-frame-content {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        height: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-menu-controls {
        display: flex;
        align-items: center;
        justify-content: center;
        flex: none;
    }

    .gds-files-toolbar-title {
        display: flex;
        flex: 1 1 0%;
        align-items: center;
        min-width: 0px;
        height: 32px;
        padding: 0px 8px;
    }

    .gds-files-toolbar-title-text {
        font-weight: 600;
        color: var(--gds-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-sidebar-content {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-sidebar-region-host {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-sidebar-empty {
        padding: 6px;
        font-size: 12px;
        color: var(--gds-muted-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-location-item {
        width: 100%;
        max-width: 100%;
        min-width: 0px;
        border-radius: 6px;
    }

    .gds-files-volume-unmounted {
        font-size: 12px;
        color: var(--gds-muted-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-nav-icon {
        flex: none;
    }

    .gds-files-workspace-outer {
        display: flex;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-workspace-column {
        position: relative;
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-workspace-frame {
        display: flex;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-settings-host {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-content-shell {
        display: flex;
        flex: 1 1 0%;
        width: 100%;
        height: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-title-text {
        min-width: 0px;
        width: 100%;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-page-actions {
        display: flex;
        align-items: center;
        gap: 8px;
        min-width: 0px;
    }

    .gds-files-page-action-strip {
        display: flex;
        align-items: center;
        gap: 4px;
    }

    .gds-files-column-controls {
        display: flex;
        align-items: center;
        gap: 4px;
    }

    .gds-files-content-focus,
    .gds-files-content-focus.gds-region-surface,
    .gds-files-content-focus.gds-region-surface[data-focus-visible="true"],
    .gds-files-content-focus.gds-region-surface[data-focus-visible="true"]:focus,
    .gds-files-content-focus.gds-region-surface[data-focus-visible="true"]:focus-visible {
        position: relative;
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        height: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
        outline: none;
    }

    .gds-files-details-host {
        display: flex;
        flex-direction: column;
        flex: none;
        height: 100%;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-details-pane {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 280px;
        min-width: 280px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-state-center {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
        align-items: center;
        justify-content: center;
        gap: 12px;
        padding: 24px;
        text-align: center;
        color: var(--gds-muted-foreground);
    }

    .gds-files-list-surface {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        border-width: 0px;
    }

    .gds-files-table-area {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        border-width: 0px;
    }

    .gds-files-state-center[data-focus-visible="true"],
    .gds-files-table-x[data-focus-visible="true"],
    .gds-files-grid-scroll[data-focus-visible="true"] {
        outline: 1px solid var(--gds-primary);
        outline-offset: -1px;
    }

    .gds-files-table-x {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow-x: hidden;
        overflow-y: hidden;
    }

    .gds-files-table-x[data-scroll-x="true"] {
        overflow-x: auto;
    }

    .gds-files-table-inner {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-height: 0px;
    }

    .gds-files-header-row {
        display: flex;
        flex: none;
        align-items: center;
        height: 30px;
        border-bottom: 1px solid var(--gds-border);
    }

    .gds-files-header-cell {
        position: relative;
        display: flex;
        flex: none;
        align-items: center;
        min-width: 0px;
        height: 100%;
    }

    .gds-files-header-button {
        display: flex;
        flex: 1 1 0%;
        align-items: center;
        justify-content: flex-start;
        gap: 4px;
        min-width: 0px;
        height: 100%;
        padding: 0px 12px;
        border-width: 0px;
        background-color: transparent;
        font-weight: 400;
        cursor: pointer;
    }

    .gds-files-header-button[data-name-col="true"] {
        padding-left: 40px;
    }

    .gds-files-header-button[data-align-end="true"] {
        justify-content: flex-end;
        text-align: right;
    }

    .gds-files-header-button[data-sortable="false"] {
        cursor: default;
    }

    .gds-files-header-button[data-sortable="true"]:hover {
        background-color: var(--gds-hover);
    }

    .gds-files-header-label {
        flex: 0 1 auto;
        min-width: 0px;
        font-size: 12px;
        font-weight: 400;
        color: var(--gds-muted-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-header-button[data-active="true"] .gds-files-header-label {
        color: var(--gds-foreground);
    }

    .gds-files-header-sort {
        display: inline-flex;
        flex-direction: column;
        flex: none;
        align-items: center;
        justify-content: center;
        width: 12px;
        height: 18px;
        color: var(--gds-muted-foreground);
        opacity: 0;
        transition: opacity var(--gds-motion-fast, 100ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }

    .gds-files-header-button:hover .gds-files-header-sort,
    .gds-files-header-button:focus-visible .gds-files-header-sort {
        opacity: 1;
    }

    .gds-files-header-sort[data-active="true"] {
        opacity: 1;
        color: var(--gds-foreground);
    }

    .gds-files-header-resize {
        position: absolute;
        top: 0px;
        right: 0px;
        bottom: 0px;
        width: 5px;
        cursor: ew-resize;
        z-index: 2;
    }

    .gds-files-header-resize:hover {
        background-color: var(--gds-primary);
    }

    .gds-files-list-host {
        position: relative;
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
    }

    .gds-files-virtual-list {
        flex: 1 1 0%;
        min-height: 0px;
    }

    .gds-files-row-wrap {
        width: 100%;
        height: 100%;
        min-width: 0px;
        padding: 2px 0px;
        box-sizing: border-box;
    }

    .gds-files-row-wrap[data-grid="true"] {
        width: auto;
        height: auto;
        padding: 0px;
    }

    .gds-files-row-content {
        display: flex;
        align-items: center;
        width: 100%;
        height: 100%;
        min-width: 0px;
        border-width: 0px;
        border-radius: 6px;
        user-select: none;
        cursor: pointer;
        transition:
            background-color var(--gds-motion-normal, 160ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1)),
            color var(--gds-motion-normal, 160ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }

    .gds-files-row-content:hover {
        background-color: var(--gds-hover);
    }

    .gds-files-row-content[data-selected="true"],
    .gds-files-row-content[data-selected="true"]:hover {
        background-color: var(--gds-selection);
        color: var(--gds-selection-foreground);
    }

    .gds-files-row-content[data-cursor="true"] {
        outline: 1px solid var(--gds-primary);
        outline-offset: -1px;
    }

    .gds-files-row-content[data-hidden-entry="true"] {
        opacity: 0.65;
    }

    .gds-files-row-content[data-ghosted="true"] {
        opacity: 0.35;
    }

    .gds-files-name-cell {
        display: flex;
        flex: none;
        align-items: center;
        height: 100%;
        gap: 10px;
        padding: 0px 12px;
        min-width: 0px;
        box-sizing: border-box;
    }

    .gds-files-row-name-group {
        display: flex;
        flex: 1 1 0%;
        align-items: center;
        gap: 6px;
        min-width: 0px;
    }

    .gds-files-row-name-text {
        flex: 0 1 auto;
        min-width: 0px;
        font-size: 13px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-cut-icon {
        flex: none;
        color: var(--gds-muted-foreground);
    }

    .gds-files-row-content[data-selected="true"] .gds-files-cut-icon {
        color: var(--gds-selection-foreground);
    }

    .gds-files-meta-cell {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: flex-start;
        height: 100%;
        min-width: 0px;
        padding: 0px 12px;
        font-size: 12px;
        color: var(--gds-muted-foreground);
        box-sizing: border-box;
    }

    .gds-files-meta-cell > *,
    .gds-files-meta-cell .zui-tooltip__trigger {
        display: flex;
        min-width: 0px;
        max-width: 100%;
        overflow: hidden;
    }

    .gds-files-meta-cell[data-align-end="true"] {
        justify-content: flex-end;
        text-align: right;
    }

    .gds-files-meta-cell[data-align-end="true"] .gds-files-meta-text {
        text-align: right;
    }

    .gds-files-row-content[data-selected="true"] .gds-files-meta-cell {
        color: var(--gds-selection-foreground);
    }

    .gds-files-meta-cell[data-current-owner="true"] {
        color: var(--gds-foreground);
    }

    .gds-files-meta-text {
        width: 100%;
        min-width: 0px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-grid-scroll {
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
        overflow-y: auto;
        overflow-x: hidden;
    }

    .gds-files-grid-pane {
        display: flex;
        flex-direction: column;
    }

    .gds-files-grid-row {
        display: flex;
        align-items: center;
        gap: 8px;
    }

    .gds-files-tile {
        position: relative;
        display: flex;
        flex-direction: column;
        align-items: center;
        padding: 8px;
        gap: 8px;
        border-radius: 8px;
        border: 1px solid transparent;
        box-sizing: border-box;
        user-select: none;
        cursor: pointer;
        transition:
            background-color var(--gds-motion-normal, 160ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1)),
            color var(--gds-motion-normal, 160ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1)),
            border-color var(--gds-motion-normal, 160ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }

    .gds-files-tile:hover {
        background-color: var(--gds-hover);
    }

    .gds-files-tile[data-selected="true"],
    .gds-files-tile[data-selected="true"]:hover {
        background-color: var(--gds-selection);
        color: var(--gds-selection-foreground);
        border-color: var(--gds-primary);
    }

    .gds-files-tile[data-cursor="true"] {
        border-color: var(--gds-primary);
    }

    .gds-files-tile[data-hidden-entry="true"] {
        opacity: 0.65;
    }

    .gds-files-tile[data-ghosted="true"] {
        opacity: 0.35;
    }

    .gds-files-tile-cut {
        position: absolute;
        top: 4px;
        right: 4px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        pointer-events: none;
        color: var(--gds-muted-foreground);
    }

    .gds-files-tile[data-selected="true"] .gds-files-tile-cut {
        color: var(--gds-selection-foreground);
    }

    .gds-files-tile-artwork {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: center;
        width: 100%;
    }

    .gds-files-tile-name-wrap {
        display: flex;
        align-items: flex-start;
        justify-content: center;
        width: 100%;
        min-width: 0px;
        overflow: hidden;
        text-align: center;
    }

    .gds-files-tile-name {
        display: block;
        width: 100%;
        min-width: 0px;
        white-space: normal;
        overflow-wrap: anywhere;
        overflow: hidden;
        text-overflow: ellipsis;
        text-align: center;
    }

    .gds-files-directory-surface {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
    }

    .gds-files-drop-background {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
    }

    .gds-files-menu-wrap {
        display: flex;
        min-width: 0px;
    }

    .gds-files-menu-anchor {
        position: relative;
        display: flex;
        align-items: center;
    }

    .gds-files-menu-item-row {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        min-width: 0px;
    }

    .gds-files-menu-item-label {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-menu-content {
        min-width: 160px;
        max-width: 340px;
        max-height: 440px;
        overflow-y: auto;
    }

    .gds-files-context-portal-anchor {
        position: fixed;
        inset: 0px;
        pointer-events: none;
    }

    .gds-files-context-point {
        position: fixed;
        width: 1px;
        height: 1px;
        pointer-events: none;
    }

    .gds-files-hidden {
        display: none;
    }

    .gds-files-empty-overlay {
        position: absolute;
        inset: 0px;
        display: flex;
        align-items: center;
        justify-content: center;
        pointer-events: none;
        color: var(--gds-muted-foreground);
    }

    .gds-files-footer-scope {
        width: 100%;
        height: 100%;
    }

    .gds-files-footer-status {
        display: flex;
        align-items: center;
        gap: 12px;
        min-width: 0px;
    }

    .gds-files-footer-status-text {
        font-size: 12px;
        color: var(--gds-muted-foreground);
        white-space: nowrap;
    }

    .gds-files-footer-controls {
        display: flex;
        flex: none;
        align-items: center;
        gap: 4px;
    }

    .gds-files-columns-dialog {
        width: 520px;
        max-width: calc(100vw - 32px);
    }

    .gds-files-columns-body {
        display: flex;
        flex-direction: column;
        gap: 12px;
    }

    .gds-files-columns-address {
        font-size: 12px;
        color: var(--gds-muted-foreground);
    }

    .gds-files-columns-list {
        display: flex;
        flex-direction: column;
        gap: 6px;
    }

    .gds-files-column-option {
        display: flex;
        align-items: center;
        gap: 8px;
    }

    .gds-files-column-name {
        flex: 1 1 0%;
        min-width: 0px;
    }

    .gds-files-about-dialog,
    .gds-files-prompt-dialog[data-about="true"],
    .gds-files-prompt-dialog[data-conflict="true"] {
        width: 520px;
        max-width: calc(100vw - 32px);
    }

    .gds-files-prompt-dialog {
        min-width: 340px;
        max-width: 520px;
    }

    .gds-files-about-box {
        display: flex;
        flex-direction: column;
        min-height: 0px;
    }

    .gds-files-confirm-body {
        display: flex;
        flex-direction: column;
        gap: 12px;
        min-width: 340px;
        max-width: 440px;
    }

    .gds-files-confirm-title-row {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 8px;
        min-width: 0px;
    }

    .gds-files-confirm-title-icon {
        flex: none;
        color: var(--gds-foreground);
    }

    .gds-files-confirm-title-icon[data-destructive="true"] {
        color: var(--gds-error);
    }

    .gds-files-confirm-items {
        display: flex;
        flex-direction: column;
        gap: 4px;
        padding: 8px 10px;
        border-radius: 6px;
        border: 1px solid var(--gds-border);
        background-color: var(--gds-surface);
        min-width: 0px;
    }

    .gds-files-confirm-item {
        font-size: 12px;
        color: var(--gds-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-confirm-more {
        font-size: 12px;
        color: var(--gds-muted-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
    "#
);

pub(super) fn local_memo<T: PartialEq + Clone + 'static>(
    compute: impl Fn() -> T + 'static,
) -> Signal<T, LocalStorage> {
    let owner = Owner::current();
    let owner_fx = owner.clone();
    let compute = Rc::new(compute);
    let compute_fx = Rc::clone(&compute);
    let slot: Rc<OnceCell<RwSignal<T, LocalStorage>>> = Rc::new(OnceCell::new());
    let slot_fx = Rc::clone(&slot);
    let fx = RenderEffect::new(move |_| {
        let next = compute_fx();
        if let Some(signal) = slot_fx.get() {
            if signal.with_untracked(|current| *current != next) {
                signal.set(next);
            }
        } else {
            let signal = match &owner_fx {
                Some(parent) => parent.with(|| RwSignal::new_local(next)),
                None => RwSignal::new_local(next),
            };
            let _ = slot_fx.set(signal);
        }
    });
    on_cleanup_local(move || drop(fx));
    let signal = *slot.get_or_init(|| match &owner {
        Some(parent) => parent.with(|| RwSignal::new_local(compute())),
        None => RwSignal::new_local(compute()),
    });
    Signal::from(signal.read_only())
}

pub(super) fn install_workspace_stylesheets() {
    gds::ui::styles::install_ui_stylesheets();
    if zgui::view::current_host().is_some() {
        install_stylesheet(WORKSPACE_SHEET, WORKSPACE_CSS);
    }
}

/// Animates content opacity `0.88 -> 1.0` over `160ms` (`motion.md`) when switching tabs, while
/// keeping initial mount and reduced-motion switches at `1.0`.
fn use_tab_enter_opacity(active_tab_id: Signal<u64, LocalStorage>) -> Signal<f32, LocalStorage> {
    #[derive(Default)]
    struct TabFadeState {
        started_at: Option<Timestamp>,
        last_frame_at: Option<Timestamp>,
        frame_handle: Option<FrameHandle>,
    }

    let opacity = RwSignal::new_local(1.0_f32);
    let timers = zgui::reactive::is_ui_thread()
        .then(Timers::current)
        .flatten();
    let state = Rc::new(RefCell::new(TabFadeState::default()));

    fn schedule_frame(
        timers: &Timers,
        state: &Rc<RefCell<TabFadeState>>,
        opacity: RwSignal<f32, LocalStorage>,
    ) {
        let timers_next = timers.clone();
        let state_next = Rc::clone(state);
        let handle = timers.request_frame(move |at| {
            let reduced =
                Gds::try_current().is_some_and(|gds| gds.appearance().motion().reduced_untracked());
            if reduced {
                let mut anim = state_next.borrow_mut();
                anim.started_at = None;
                anim.last_frame_at = None;
                anim.frame_handle = None;
                drop(anim);
                opacity.set(1.0);
                return;
            }

            let (value, done) = {
                let mut anim = state_next.borrow_mut();
                let start = match anim.started_at {
                    Some(start) => start,
                    None => {
                        let base = anim.last_frame_at.unwrap_or(at);
                        anim.started_at = Some(base);
                        base
                    }
                };
                anim.last_frame_at = Some(at);
                let elapsed = (at - start).as_secs_f32();
                let duration = (WORKSPACE_DURATION_MS as f32) / 1000.0;
                let progress = (elapsed / duration).clamp(0.0, 1.0);
                if progress >= 1.0 {
                    anim.started_at = None;
                    anim.last_frame_at = None;
                    anim.frame_handle = None;
                    (1.0, true)
                } else {
                    let eased = ease_out_cubic(progress);
                    let current =
                        TAB_ENTER_INITIAL_OPACITY + (1.0 - TAB_ENTER_INITIAL_OPACITY) * eased;
                    (current, false)
                }
            };
            opacity.set(value);
            if !done {
                schedule_frame(&timers_next, &state_next, opacity);
            }
        });
        state.borrow_mut().frame_handle = Some(handle);
    }

    let watch_state = Rc::clone(&state);
    let tab_fx = RenderEffect::new(move |prev_tab: Option<u64>| {
        let current = active_tab_id.get();
        let Some(prev) = prev_tab else {
            return current;
        };
        if prev == current {
            return current;
        }
        let reduced =
            Gds::try_current().is_some_and(|gds| gds.appearance().motion().reduced_untracked());
        if reduced || timers.is_none() {
            let old = {
                let mut anim = watch_state.borrow_mut();
                anim.started_at = None;
                anim.last_frame_at = None;
                anim.frame_handle.take()
            };
            drop(old);
            opacity.set(1.0);
            return current;
        }
        if let Some(ref timers) = timers {
            let old = {
                let mut anim = watch_state.borrow_mut();
                anim.started_at = None;
                anim.last_frame_at = None;
                anim.frame_handle.take()
            };
            drop(old);
            opacity.set(TAB_ENTER_INITIAL_OPACITY);
            schedule_frame(timers, &watch_state, opacity);
        }
        current
    });

    on_cleanup_local(move || {
        drop(tab_fx);
        let old = state.borrow_mut().frame_handle.take();
        drop(old);
    });

    opacity.into()
}

#[derive(Clone, PartialEq)]
enum FileBody {
    Loading(Location),
    Empty(&'static str),
    Network,
    Collection(super::surfaces::CollectionBodyState),
    List,
    Grid,
}

impl Browser {
    pub fn view(&self) -> impl IntoView + use<> {
        let browser = self.clone();
        Scoped::new(move || browser.view_scoped())
    }

    fn view_scoped(&self) -> impl IntoView + use<> {
        install_workspace_stylesheets();

        let keytip_active = RwSignal::new_local(false);
        let hint_context = self.clone();
        let keytip_guard_fx = RenderEffect::new(move |_| {
            let menu_open = !matches!(hint_context.0.menus.surface.get(), Surface::Closed);
            if menu_open && keytip_active.get_untracked() {
                keytip_active.set(false);
            }
        });
        on_cleanup_local(move || drop(keytip_guard_fx));

        // `WindowHandle::size()` is already in `CssPx` and starts `(0.0, 0.0)` before Wayland
        // configure; ignore non-positive widths and do not divide by scale (#114).
        if let Some(window) = self.0.window.clone() {
            let width_sig = self.0.window_width;
            let size = window.size();
            let win_fx = RenderEffect::new(move |_| {
                let width = size.get().width.0;
                if width.is_finite() && width > 0.0 {
                    width_sig.set(width);
                }
            });
            on_cleanup_local(move || drop(win_fx));
        } else {
            let root = self.0.root_focus;
            let width_sig = self.0.window_width;
            let root_size = root.observe_border_size();
            let root_fx = RenderEffect::new(move |_| {
                if let Some(sz) = root_size.get() {
                    let s = root.scale().max(1.0);
                    let width = sz.width.0 / s;
                    if width.is_finite() && width > 0.0 {
                        width_sig.set(width);
                    }
                }
            });
            on_cleanup_local(move || drop(root_fx));
        }

        let help = local_memo({
            let browser = self.clone();
            move || browser.pane() == Pane::Help
        });

        let root_focus = self.0.root_focus;
        let notice_sig = self.0.notice;
        let prompt_sig = self.0.prompt;
        let prompt_about = self.0.prompt_about;
        let prompt_open = Signal::derive_local(move || !matches!(prompt_sig.get(), Prompt::Closed));
        let prompt_conflict = RwSignal::new_local(false);
        let prompt_about_retained = RwSignal::new_local(false);
        let decision_slot: PromptDecisionHandler = Rc::new(RefCell::new(None));
        let key_decision_slot = Rc::clone(&decision_slot);

        let keyboard = self.clone();
        let mouse = self.clone();
        let main_browser = self.clone();
        let prompt_browser = self.clone();
        let cancel_browser = self.clone();
        let connector_view = self.0.connector.view();
        let menu_overlay = self.menu_overlay();

        view! {
            KeytipHost(active = Some(keytip_active)) {
                NotificationHost {
                    {{
                        let notice_fx = RenderEffect::new(move |_| {
                            if let Some(message) = notice_sig.get() {
                                let _z = enter_non_reactive_zone();
                                notify_error(message);
                            }
                        });
                        on_cleanup_local(move || drop(notice_fx));

                        let prompt_dialog = Scoped::new(move || {
                            let _prompt_modal_scope = DialogScope::use_open(prompt_open);
                            view! {
                                Dialog(
                                    open = Binding::controlled(
                                        prompt_open,
                                        move |next: bool| {
                                            if !next {
                                                cancel_browser.cancel_prompt();
                                            }
                                        }
                                    )
                                ) {
                                    DialogContent(
                                        class = "gds-files-prompt-dialog",
                                        attr:data-about = move || {
                                            (prompt_about.get() || prompt_about_retained.get())
                                                .then(|| "true".to_owned())
                                        },
                                        attr:data-conflict = move || {
                                            prompt_conflict.get().then(|| "true".to_owned())
                                        },
                                        dismiss_on_outside_press = true,
                                        dismiss_on_escape = true,
                                        on:key_down = move |ev: &mut EventCx<'_, events::KeyDown>| {
                                            let handler = key_decision_slot.borrow().clone();
                                            if let Some((disabled, cb)) = handler {
                                                handle_dialog_key_down(disabled.get(), cb, ev);
                                            }
                                        }
                                    ) {
                                        {{
                                            let pb = prompt_browser.clone();
                                            let gen_sig = pb.0.prompt_generation;
                                            let opts_sig = pb.0.confirm_options;
                                            let retained = Rc::new(RefCell::new((
                                                Prompt::Closed,
                                                ConfirmOptions::default(),
                                            )));
                                            let slot = Rc::clone(&decision_slot);
                                            move || {
                                                let generation = gen_sig.get();
                                                let live_options = opts_sig.get();
                                                let _z = enter_non_reactive_zone();
                                                let current = pb.0.prompt.get_untracked();
                                                let (prompt, options) = if matches!(current, Prompt::Closed) {
                                                    retained.borrow().clone()
                                                } else {
                                                    let is_about = matches!(current, Prompt::About);
                                                    let is_conflict = matches!(
                                                        &current,
                                                        Prompt::Name { form, .. } if form.is_conflict()
                                                    );
                                                    prompt_about_retained.set(is_about);
                                                    prompt_conflict.set(is_conflict);
                                                    *retained.borrow_mut() =
                                                        (current.clone(), live_options.clone());
                                                    (current, live_options)
                                                };
                                                pb.prompt_view(prompt, options, generation, &slot)
                                            }
                                        }}
                                    }
                                }
                            }
                        });

                        view! {
                            column(
                                node_ref = root_focus,
                                class = "gds-files-root",
                                tabindex = Focus::Sequential,
                                on:key_down = move |ev: &mut EventCx<'_, events::KeyDown>| {
                                    keyboard.key_down(ev);
                                },
                                on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                                    mouse.mouse_navigation(ev);
                                }
                            ) {
                                {{move || {
                                    if help.get() {
                                        main_browser.help_view()
                                    } else {
                                        main_browser.workspace_view()
                                    }
                                }}}
                                {connector_view}
                                {menu_overlay}
                                {prompt_dialog}
                            }
                        }
                    }}
                }
            }
        }
    }

    fn help_view(&self) -> AnyView {
        let root_ref = self.0.settings_node;
        let back_click = self.clone();
        let back_button = AnyView::new(view! {
            row(class = "gds-files-menu-controls") {
                IconButton(
                    icon = IconName::ArrowLeft,
                    label = Signal::derive_local(|| i18n::text("page-return")),
                    shortcut = Some(Command::Cancel.shortcut()),
                    on:click = move |_| back_click.back_page()
                )
            }
        });
        let title_controls = AnyView::new(view! {
            WindowDragArea(class = "gds-files-toolbar-title") {
                text(class = "gds-files-toolbar-title-text") {{i18n::text("help")}}
            }
        });
        let help_body = AnyView::new(view! {
            box(
                node_ref = root_ref,
                tabindex = Focus::Sequential,
                class = "gds-files-settings-host"
            ) {
                FilesHelp()
            }
        });
        let empty_status = AnyView::new(view! { box() });

        AnyView::new(view! {
            Frame(without_sidebar = true, class = "gds-files-frame") {
                Toolbar(
                    menu = back_button,
                    controls = title_controls,
                    window_controls = AnyView::new(view! { WindowControls() })
                ) {}
                WorkspaceFrame(
                    content = help_body,
                    paint_content_surface = true,
                    class = "gds-files-workspace-frame"
                ) {}
                Footer(status = empty_status) {}
            }
        })
    }

    fn workspace_view(&self) -> AnyView {
        let browser = self.clone();
        AnyView::new(Scoped::new(move || browser.workspace_view_scoped()))
    }

    fn workspace_view_scoped(&self) -> AnyView {
        let pane_memo = local_memo({
            let browser = self.clone();
            move || browser.pane()
        });
        let sidebar_width = self.sidebar_width();
        let sidebar_pres = self.sidebar_presentation();

        let workspace_col = NodeRef::new();
        let workspace_size = workspace_col.observe_border_size();
        let resize_browser = self.clone();
        let term_size_fx = RenderEffect::new(move |_| {
            if let Some(sz) = workspace_size.get() {
                let s = workspace_col.scale().max(1.0);
                let w = sz.width.0 / s;
                let h = sz.height.0 / s;
                if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
                    resize_browser.terminal_workspace_resized(w, h);
                }
            }
        });
        on_cleanup_local(move || drop(term_size_fx));

        let picker_is_none = Signal::derive_local({
            let browser = self.clone();
            move || browser.read(|state| state.picker.is_none())
        });

        let sidebar_browser = self.clone();
        let content_browser = self.clone();
        let terminal_browser = self.clone();
        let status_browser = self.clone();
        let footer_ctrl_browser = self.clone();

        let toolbar_left = self.toolbar_leading();
        let toolbar_controls = self.toolbar_controls();
        let sidebar_scope = AnyView::new(view! { box(class = "gds-files-footer-scope") });
        let footer_status = AnyView::new(view! {
            {{move || {
                if pane_memo.get() == Pane::Files {
                    let sb = status_browser.clone();
                    if let Some(picker_name) = sb.render_picker_name() {
                        picker_name
                    } else {
                        let status_controls = sb.status_controls();
                        AnyView::new(view! {
                            row(class = "gds-files-footer-status") {
                                text(class = "gds-files-footer-status-text") {{move || sb.status()}}
                                {status_controls}
                            }
                        })
                    }
                } else {
                    status_browser.0.settings.footer_text()
                }
            }}}
        });
        let footer_controls = AnyView::new(view! {
            Show(when = move || pane_memo.get() == Pane::Files) {
                {{
                    footer_ctrl_browser
                        .render_picker_controls()
                        .unwrap_or_else(|| footer_ctrl_browser.footer_controls())
                }}
            }
        });

        AnyView::new(view! {
            Frame(sidebar_width = sidebar_width, class = "gds-files-frame") {
                Toolbar(
                    left = toolbar_left,
                    controls = toolbar_controls,
                    window_controls = AnyView::new(view! { WindowControls() })
                ) {}
                {{move || match pane_memo.get() {
                    Pane::Settings => {
                        let sb = sidebar_browser.clone();
                        let cb = content_browser.clone();
                        let pres_initial = sidebar_pres.get_untracked();
                        let settings_sidebar = sb.0.settings.sidebar(pres_initial);
                        let settings_content = AnyView::new(view! {
                            box(
                                node_ref = cb.0.settings_node,
                                tabindex = Focus::Sequential,
                                class = "gds-files-settings-host"
                            ) {
                                {{cb.0.settings.content()}}
                            }
                        });
                        AnyView::new(view! {
                            WorkspaceFrame(
                                sidebar = settings_sidebar,
                                content = settings_content,
                                paint_content_surface = true,
                                class = "gds-files-workspace-frame"
                            ) {}
                        })
                    }
                    Pane::Files => {
                        let pres_initial = sidebar_pres.get_untracked();
                        let files_sidebar = sidebar_browser.sidebar(pres_initial);
                        let files_content = content_browser.file_content();
                        let term_b = terminal_browser.clone();
                        AnyView::new(view! {
                            WorkspaceFrame(
                                paint_content_surface = false,
                                class = "gds-files-workspace-outer"
                            ) {
                                column(node_ref = workspace_col, class = "gds-files-workspace-column") {
                                    WorkspaceFrame(
                                        sidebar = files_sidebar,
                                        content = files_content,
                                        paint_content_surface = true,
                                        class = "gds-files-workspace-frame"
                                    ) {}
                                    Show(when = move || picker_is_none.get()) {
                                        {{term_b.terminal_view(pres_initial)}}
                                    }
                                }
                            }
                        })
                    }
                    Pane::Help => AnyView::new(view! { box(class = "gds-files-hidden") }),
                }}}
                Footer(
                    sidebar_status = sidebar_scope,
                    status = footer_status,
                    controls = footer_controls
                ) {}
            }
        })
    }

    fn footer_controls(&self) -> AnyView {
        let terminal_sig = self.0.terminal;
        let click_browser = self.clone();
        let terminal_toggled = Signal::derive_local(move || terminal_sig.get());
        let terminal_label = Signal::derive_local(move || {
            i18n::text(if terminal_sig.get() {
                "files-terminal-hide"
            } else {
                "files-terminal-show"
            })
        });
        let visible = Signal::derive_local({
            let browser = self.clone();
            move || {
                browser.read(|state| state.picker.is_none())
                    && browser.0.preferences.state().with(|p| p.terminal_enabled())
            }
        });

        AnyView::new(view! {
            row(class = "gds-files-footer-controls") {
                Show(when = move || visible.get()) {
                    {{
                        let clk_b = click_browser.clone();
                        view! {
                            IconButton(
                                icon = IconName::Terminal,
                                label = terminal_label,
                                toggled = Some(terminal_toggled),
                                shortcut = Some(Command::Terminal.shortcut()),
                                a11y:role = Role::Button,
                                on:click = move |_| clk_b.toggle_terminal()
                            )
                        }
                    }}
                }
            }
        })
    }

    fn control(
        &self,
        icon: IconName,
        label: &'static str,
        action: fn(&Browser),
        command: Option<Command>,
        active: Option<Signal<bool, LocalStorage>>,
    ) -> AnyView {
        let click_browser = self.clone();
        let shortcut = command.map(Command::shortcut);
        let active_sig = active.unwrap_or_else(|| Signal::stored_local(false));

        AnyView::new(view! {
            IconButton(
                icon = icon,
                label = Signal::derive_local(move || i18n::text(label)),
                active = active_sig,
                shortcut = shortcut,
                on:click = move |_| action(&click_browser)
            )
        })
    }

    fn page_title(&self) -> String {
        self.read(|state| {
            let location = state.tab().navigation.location();
            if *location == Location::from(state.home.as_path()) {
                return i18n::text("home");
            }
            if let Location::Volume { identity, relative } = location
                && relative.is_empty()
                && let Some(volume) = self.volume_at_location(identity)
            {
                return volume.name;
            }
            self.location_title(location)
        })
    }

    fn details_available(&self) -> bool {
        self.read(|state| {
            let tab = state.tab();
            match tab.contents {
                Contents::Directory | Contents::Places(_) => true,
                Contents::Tags { .. } => !tab.is_pure_tag_catalogue(),
                Contents::LoadingCollection
                | Contents::Unavailable(_)
                | Contents::Network { .. }
                | Contents::Trash(_)
                | Contents::Remote { .. }
                | Contents::Volumes => false,
            }
        })
    }

    fn file_content(&self) -> AnyView {
        let browser = self.clone();
        AnyView::new(Scoped::new(move || browser.file_content_scoped()))
    }

    fn file_content_scoped(&self) -> AnyView {
        let title_browser = self.clone();
        let content_browser = self.clone();
        let details_browser = self.clone();
        let actions_browser = self.clone();
        let notice_browser = self.clone();
        let focus_browser = self.clone();
        let search_text_browser = self.clone();
        let search_ime_browser = self.clone();

        let content_ref = self.0.content_focus;
        let width_sig = self.0.content_width;
        let size_obs = content_ref.observe_border_size();
        let size_fx = RenderEffect::new(move |_| {
            if let Some(sz) = size_obs.get() {
                let s = content_ref.scale().max(1.0);
                let width = sz.width.0 / s;
                if width.is_finite() && width > 0.0 {
                    width_sig.set(width);
                }
            }
        });
        on_cleanup_local(move || drop(size_fx));

        let region_attrs =
            surface_with_shortcut(content_ref, Some(Command::FocusContent.shortcut()));
        let keyboard_modality = use_keyboard_modality(content_ref);
        let visible_sig = self.0.list_focus_visible;
        {
            let sync_browser = focus_browser.clone();
            let focus_sync_fx = RenderEffect::new(move |_| {
                let direct_ring = is_focus_visible(content_ref);
                let is_kbd = keyboard_modality.get();
                if let Some(host) = current_host() {
                    let active = host.focused().get();
                    let self_id = content_ref.get();
                    let direct = self_id.is_some() && active == self_id;
                    let inside =
                        active.is_some_and(|id| self_id == Some(id) || content_ref.contains(id));
                    sync_browser.0.list_focused.set(inside);
                    if direct_ring {
                        if !visible_sig.get_untracked() {
                            visible_sig.set(true);
                        }
                    } else if (!direct || !is_kbd) && visible_sig.get_untracked() {
                        visible_sig.set(false);
                    }
                } else if visible_sig.get_untracked() != direct_ring {
                    visible_sig.set(direct_ring);
                }
            });
            on_cleanup_local(move || drop(focus_sync_fx));
        }

        let active_tab_id = local_memo({
            let browser = self.clone();
            move || browser.read_structure(|state| state.tab().id)
        });
        let tab_opacity = use_tab_enter_opacity(active_tab_id);

        let body = local_memo({
            let browser = self.clone();
            move || browser.file_body()
        });
        let is_network = local_memo(move || matches!(body.get(), FileBody::Network));

        let details_goal = Signal::derive_local({
            let browser = self.clone();
            move || {
                if !browser.compact() && browser.0.details.get() && browser.details_available() {
                    1.0_f32
                } else {
                    0.0_f32
                }
            }
        });
        let details_progress = use_transition(details_goal, WORKSPACE_DURATION_MS);
        let details_mounted = Signal::derive_local({
            let browser = self.clone();
            move || !browser.compact() && details_progress.get() > 0.001
        });

        let filter_node = self.0.filter_node;
        let filter_sig = self.0.filter;
        let append_search = move |browser: &Browser, inserted: &str| {
            if inserted.is_empty()
                || inserted.chars().all(char::is_control)
                || !browser.filter_available_untracked()
                || !browser.0.preferences.snapshot().files.auto_focus_search
                || focus_is_text_entry()
            {
                return false;
            }
            let mut next = filter_sig.get_untracked();
            next.push_str(inserted);
            filter_sig.set(next.clone());
            browser.filter(next.clone());
            filter_node.focus();
            filter_node.set_value(&next);
            true
        };
        let append_ime = append_search;

        AnyView::new(view! {
            row(class = "gds-files-content-shell") {
                box(
                    node_ref = content_ref,
                    class = "gds-files-content-focus",
                    tabindex = Focus::Sequential,
                    style:opacity = move || {
                        let alpha = tab_opacity.get();
                        (alpha < 0.999).then(|| format!("{alpha:.3}"))
                    },
                    {..region_attrs},
                    on:text = move |ev: &mut EventCx<'_, events::Text>| {
                        if append_search(&search_text_browser, ev.text.as_str()) {
                            ev.prevent_default();
                            ev.stop_propagation();
                        }
                    },
                    on:ime_commit = move |ev: &mut EventCx<'_, events::ImeCommit>| {
                        if let zgui::vocab::ImeEvent::Commit(committed) = &**ev
                            && append_ime(&search_ime_browser, committed.as_str())
                        {
                            ev.prevent_default();
                            ev.stop_propagation();
                        }
                    }
                ) {
                    {{move || {
                        if is_network.get() {
                            // `surfaces/network.rs` renders its own desktop `ContentPage` (`#121`).
                            let net_browser = content_browser.clone();
                            let compact_net = content_browser.clone();
                            let tb_net = title_browser.clone();
                            let ab_net = actions_browser.clone();
                            let picker_footer = net_browser.picker_footer();
                            AnyView::new(view! {
                                column(class = "gds-files-workspace-column") {
                                    Show(when = move || compact_net.compact()) {
                                        {{
                                            let tb_hdr = tb_net.clone();
                                            let ab_hdr = ab_net.clone();
                                            view! {
                                                PageHeader {
                                                    PageTitle {
                                                        text(class = "gds-files-title-text") {{move || tb_hdr.page_title()}}
                                                    }
                                                    PageActions(class = "gds-files-page-actions") {
                                                        {move || ab_hdr.page_actions()}
                                                    }
                                                }
                                            }
                                        }}
                                    }
                                    {{net_browser.network_content()}}
                                    {picker_footer}
                                }
                            })
                        } else {
                            let tb = title_browser.clone();
                            let ab = actions_browser.clone();
                            let nb = notice_browser.clone();
                            let cb = content_browser.clone();
                            let filter_view = cb.folder_filter();
                            let picker_footer = cb.picker_footer();
                            AnyView::new(view! {
                                ContentPage {
                                    PageHeader {
                                        PageTitle {
                                            text(class = "gds-files-title-text") {{move || tb.page_title()}}
                                        }
                                        PageActions(class = "gds-files-page-actions") {
                                            {filter_view}
                                            {move || ab.page_actions()}
                                        }
                                    }
                                    {move || nb.collection_notice()}
                                    PageContent {
                                        {{move || {
                                            let current = body.get();
                                            let _z = enter_non_reactive_zone();
                                            cb.render_file_body(current)
                                        }}}
                                        {picker_footer}
                                    }
                                }
                            })
                        }
                    }}}
                }
                Show(when = move || details_mounted.get()) {
                    {{
                        let d_browser = details_browser.clone();
                        view! {
                            box(
                                class = "gds-files-details-host",
                                style:width = move || {
                                    Some(format!("{:.1}px", (DETAILS_WIDTH * details_progress.get()).max(0.0)))
                                }
                            ) {
                                box(class = "gds-files-details-pane") {
                                    {{d_browser.details_view()}}
                                }
                            }
                        }
                    }}
                }
            }
        })
    }

    fn file_body(&self) -> FileBody {
        if let Some(location) = self.read(|state| {
            let tab = state.tab();
            tab.navigation.is_loading().then(|| {
                tab.navigation
                    .pending_location()
                    .unwrap_or_else(|| tab.navigation.location())
                    .clone()
            })
        }) {
            return FileBody::Loading(location);
        }
        if let Some(collection) = self.collection_body_state() {
            return FileBody::Collection(collection);
        }
        self.read(|state| {
            let tab = state.tab();
            match &tab.contents {
                Contents::Remote { .. } if tab.rows.is_empty() && tab.navigation.is_loading() => {
                    FileBody::Loading(tab.navigation.location().clone())
                }
                Contents::Network { .. } => FileBody::Network,
                _ => {
                    if tab.rows.is_empty() {
                        FileBody::Empty(if self.0.filter.with(String::is_empty) {
                            "empty-directory"
                        } else {
                            "empty-filter"
                        })
                    } else if tab.mode == ViewMode::Grid {
                        FileBody::Grid
                    } else {
                        FileBody::List
                    }
                }
            }
        })
    }

    fn render_file_body(&self, body: FileBody) -> AnyView {
        match body {
            FileBody::Loading(location) => self.location_loading(&location),
            FileBody::Collection(state) => state.view(self),
            FileBody::Network => self.network_content(),
            FileBody::Empty(message) => {
                let target = self.empty_view(message);
                self.directory_menu(self.background_drop_target(target))
            }
            FileBody::List => self.list_view(),
            FileBody::Grid => self.grid_view(),
        }
    }

    fn location_loading(&self, location: &Location) -> AnyView {
        let message = match location {
            Location::Volume { identity, .. } => {
                let label = self
                    .volume_at_location(identity)
                    .map_or_else(|| location.label(), |volume| volume.name);
                gds::t!("loading-volume", name = label)
            }
            _ => i18n::text("loading"),
        };
        let cancel = self.clone();
        AnyView::new(view! {
            column(class = "gds-files-state-center") {
                Spinner()
                text {{message}}
                Button(
                    variant = ButtonVariant::Outline,
                    on:click = move |_| cancel.cancel_location_load(),
                ) {{i18n::text("operation-cancel")}}
            }
        })
    }

    fn empty_view(&self, message: &'static str) -> AnyView {
        let focus_visible = self.0.list_focus_visible;
        AnyView::new(view! {
            PageTableArea(class = "gds-files-table-area") {
                column(
                    class = "gds-files-state-center",
                    attr:data-focus-visible = move || focus_visible.get().then(|| "true".to_owned())
                ) {
                    text {{i18n::text(message)}}
                }
            }
        })
    }

    fn column_width(&self, column: Column) -> f32 {
        let explicit = self
            .0
            .column_widths
            .with(|widths| widths.get(&column).copied());
        if let Some(width) = explicit {
            return width;
        }
        let viewport = self.table_viewport_width();
        let columns = self.read(|state| state.tab().columns.columns().to_vec());
        let weights: f32 = columns
            .iter()
            .map(|col| Self::column_weight(*col))
            .sum::<f32>()
            .max(1.0);
        if columns.last() == Some(&column) {
            let preceding: f32 = columns
                .iter()
                .take(columns.len().saturating_sub(1))
                .map(|col| (viewport * Self::column_weight(*col) / weights).floor())
                .sum();
            (viewport - preceding).max(60.0)
        } else {
            (viewport * Self::column_weight(column) / weights)
                .floor()
                .max(60.0)
        }
    }

    fn column_weight(column: Column) -> f32 {
        match column {
            Column::Name => 4.2,
            Column::DetailedType => 1.8,
            Column::Modified | Column::Accessed => 1.6,
            Column::Type => 1.3,
            Column::Size | Column::Owner | Column::Group | Column::Permissions => 1.0,
        }
    }

    fn content_columns_width(&self) -> f32 {
        if let Some(columns) = self.collection_columns() {
            return columns.into_iter().map(|(_, width)| width).sum::<f32>();
        }
        let has_explicit = self.0.column_widths.with(|widths| !widths.is_empty());
        if !has_explicit {
            return self.table_viewport_width();
        }
        self.read(|state| {
            state
                .tab()
                .columns
                .columns()
                .iter()
                .map(|column| self.column_width(*column))
                .sum::<f32>()
        })
    }

    fn table_width(&self) -> f32 {
        self.content_columns_width()
            .max(self.table_viewport_width())
    }

    fn column_header(&self, column: Column) -> AnyView {
        let width_browser = self.clone();
        let sort_browser = self.clone();
        let resize_drag = self.clone();
        let resize_cancel = self.clone();
        let resize_end = self.clone();
        let sortable = self.sortable_columns().contains(&column);
        let sorting = self.read(|state| state.tab().sort);
        let active = sortable && sorting.key == column;
        let ascending = sorting.direction == Direction::Ascending;
        let align_end = column == Column::Size;

        let drag_state: RwSignal<Option<(f32, f32)>, LocalStorage> = RwSignal::new_local(None);
        let start_width = RwSignal::new_local(self.column_width(column));
        let window = try_use_window();

        let sort_indicator = sortable.then(|| {
            AnyView::new(view! {
                box(
                    class = "gds-files-header-sort",
                    attr:data-active = active.then(|| "true".to_owned()),
                    a11y:hidden = true
                ) {
                    {if active {
                        let icon = if ascending {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        };
                        AnyView::new(view! {
                            Icon(name = icon, size = 12.0)
                        })
                    } else {
                        AnyView::new(view! {
                            column(style:align-items = "center", style:justify-content = "center") {
                                Icon(name = IconName::ChevronUp, size = 8.0) {}
                                Icon(name = IconName::ChevronDown, size = 8.0) {}
                            }
                        })
                    }}
                }
            })
        });

        let label_text = i18n::text(column.label_id());
        let label_node = AnyView::new(view! {
            text(class = "gds-files-header-label") {{label_text.clone()}}
        });

        AnyView::new(view! {
            row(
                class = "gds-files-header-cell",
                style:width = move || Some(format!("{:.1}px", width_browser.column_width(column)))
            ) {
                control(
                    class = "gds-files-header-button",
                    attr:data-name-col = (column == Column::Name).then(|| "true".to_owned()),
                    attr:data-align-end = align_end.then(|| "true".to_owned()),
                    attr:data-sortable = if sortable { "true" } else { "false" },
                    attr:data-active = active.then(|| "true".to_owned()),
                    tabindex = if sortable { Focus::Sequential } else { Focus::Programmatic },
                    a11y:role = Role::Button,
                    a11y:label = SharedString::from(label_text),
                    a11y:disabled = !sortable,
                    on:click = move |_| {
                        if sortable {
                            sort_browser.sort_by(column);
                        }
                    }
                ) {
                    {if align_end {
                        AnyView::new(view! {
                            {sort_indicator}
                            {label_node}
                        })
                    } else {
                        AnyView::new(view! {
                            {label_node}
                            {sort_indicator}
                        })
                    }}
                }
                box(
                    class = "gds-files-header-resize",
                    on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                        if ev.button != Some(PointerButton::Primary) || !ev.primary {
                            return;
                        }
                        let w = resize_drag.column_width(column);
                        start_width.set(w);
                        drag_state.set(Some((ev.position.x.0, w)));
                        ev.capture_pointer();
                        ev.stop_propagation();
                    },
                    on:pointer_move = {
                        let b = self.clone();
                        move |ev: &mut EventCx<'_, events::PointerMove>| {
                            let Some((start_x, base_w)) = drag_state.get_untracked() else {
                                return;
                            };
                            let delta = ev.position.x.0 - start_x;
                            b.0.column_widths.update(|widths| {
                                widths.insert(column, (base_w + delta).clamp(60.0, 900.0));
                            });
                        }
                    },
                    on:pointer_up = move |ev: &mut EventCx<'_, events::PointerUp>| {
                        if drag_state.get_untracked().is_some() {
                            drag_state.set(None);
                            ev.release_pointer();
                            let widths = resize_end
                                .0
                                .column_widths
                                .get_untracked()
                                .into_iter()
                                .map(|(col, width)| (col.id().to_owned(), width))
                                .collect();
                            resize_end.update(|state| {
                                state
                                    .places
                                    .set_widths(state.tab().navigation.location().clone(), widths);
                            });
                            resize_end.save_places();
                        }
                    },
                    on:pointer_cancel = move |ev: &mut EventCx<'_, events::PointerCancel>| {
                        if drag_state.get_untracked().is_some() {
                            drag_state.set(None);
                            ev.release_pointer();
                            let original = start_width.get_untracked();
                            resize_cancel.0.column_widths.update(|widths| {
                                widths.insert(column, original);
                            });
                        }
                    },
                    on:pointer_enter = {
                        let win = window.clone();
                        move |_| {
                            if let Some(ref w) = win {
                                w.set_cursor(CursorStyle::ResizeEastWest);
                            }
                        }
                    },
                    on:pointer_leave = {
                        let win = window;
                        move |_| {
                            if drag_state.get_untracked().is_none()
                                && let Some(ref w) = win
                            {
                                w.set_cursor(CursorStyle::Default);
                            }
                        }
                    }
                )
            }
        })
    }

    fn attach_list_scroll_observer(&self) {
        let list_node = self.0.list;
        let scroll_obs = list_node.observe_scroll();
        let browser = self.clone();
        let scroll_fx = RenderEffect::new(move |_| {
            let pos = scroll_obs.get();
            if pos.scrollport.width.0 > 0.0 || pos.scrollport.height.0 > 0.0 {
                let s = list_node.scale().max(1.0);
                browser.viewport_changed(Point::new(
                    CssPx(pos.offset.x.0 / s),
                    CssPx(pos.offset.y.0 / s),
                ));
            }
        });
        on_cleanup_local(move || drop(scroll_fx));
    }

    fn read_structure<R>(&self, read: impl FnOnce(&super::model::State) -> R) -> R {
        let _ = self.0.revision.get();
        read(&self.0.state.borrow())
    }

    fn clear_blank_selection(&self) {
        self.update_selection(|state| {
            let view = state.tab_mut().navigation.view_mut();
            view.selection.clear();
            view.anchor = None;
        });
    }

    fn list_view(&self) -> AnyView {
        let browser = self.clone();
        AnyView::new(Scoped::new(move || browser.list_view_scoped()))
    }

    fn list_view_scoped(&self) -> AnyView {
        self.attach_list_scroll_observer();

        let list_node = self.0.list;
        let viewport = NodeRef::new();
        let count_browser = self.clone();
        let rows_browser = self.clone();
        let header_browser = self.clone();
        let table_browser = self.clone();
        let overflow_browser = self.clone();

        let row_count = Signal::derive_local(move || {
            count_browser.read_structure(|state| state.tab().rows.len())
        });
        let header_h_sig = Signal::derive_local(move || {
            if row_count.get() == 0 {
                0.0
            } else {
                HEADER_HEIGHT
            }
        });
        let empty = local_memo({
            let browser = self.clone();
            move || browser.collection_empty_label()
        });

        let has_marquee =
            self.read_untracked(|state| state.picker.is_none() && state.allows_multiple());
        let blank_press = (!has_marquee).then(|| {
            let clear_browser = self.clone();
            UnsyncCallback::new(move |()| clear_browser.clear_blank_selection())
        });

        let focus_visible = self.0.list_focus_visible;
        let target = AnyView::new(view! {
            ListSurface(
                scroll_ref = list_node,
                rows = row_count,
                row_height = ROW_HEIGHT,
                header_height = header_h_sig,
                inset_x = TABLE_INSET,
                on_blank_press = blank_press,
                class = "gds-files-list-surface"
            ) {
                PageTableArea(class = "gds-files-table-area") {
                    scroll(
                        node_ref = viewport,
                        class = "gds-files-table-x",
                        attr:data-focus-visible = move || {
                            focus_visible.get().then(|| "true".to_owned())
                        },
                        attr:data-scroll-x = move || {
                            (overflow_browser.content_columns_width()
                                > overflow_browser.table_viewport_width() + 0.5)
                                .then(|| "true".to_owned())
                        }
                    ) {
                        column(
                            class = "gds-files-table-inner",
                            style:min-width = move || Some(format!("{:.1}px", table_browser.table_width()))
                        ) {
                            {{move || {
                                if let Some(header) = header_browser.collection_header() {
                                    header
                                } else {
                                    let columns = header_browser
                                        .read_structure(|state| state.tab().columns.clone());
                                    let cells = columns
                                        .columns()
                                        .iter()
                                        .map(|column| header_browser.column_header(*column))
                                        .collect::<Vec<_>>();
                                    AnyView::new(view! {
                                        row(
                                            class = "gds-files-header-row",
                                            style:height = format!("{HEADER_HEIGHT}px")
                                        ) {{cells}}
                                    })
                                }
                            }}}
                            box(class = "gds-files-list-host") {
                                VirtualList(
                                    node_ref = Some(list_node),
                                    count = row_count,
                                    row_size = ROW_HEIGHT,
                                    label = i18n::text("app-title"),
                                    class = "gds-files-virtual-list",
                                    row = move |index: usize| rows_browser.visible_row(index, None)
                                ) {}
                                {{move || empty.get().map(|label| AnyView::new(view! {
                                    box(class = "gds-files-empty-overlay") {
                                        text {{i18n::text(label)}}
                                    }
                                }))}}
                            }
                        }
                    }
                }
            }
        });

        let dropped = self.background_drop_target(target);
        let surface = if has_marquee {
            self.marquee_surface(dropped, marquee::Layout::List { viewport })
        } else {
            dropped
        };
        self.directory_menu(surface)
    }

    fn grid_view(&self) -> AnyView {
        let browser = self.clone();
        AnyView::new(Scoped::new(move || browser.grid_view_scoped()))
    }

    fn grid_view_scoped(&self) -> AnyView {
        self.attach_list_scroll_observer();

        let list_node = self.0.list;
        let motion = GridMotion::new(self);
        let motion_rows = motion.clone();
        let count_browser = self.clone();
        let rows_browser = self.clone();

        let grid_row_count = Signal::derive_local(move || {
            let total = count_browser.read_structure(|state| state.tab().rows.len());
            let columns = motion.target.get().columns.max(1);
            total.div_ceil(columns)
        });
        let row_height = Signal::derive_local(move || motion.target.get().row_height.max(1.0));
        let seen = Virtualize::new(list_node, grid_row_count, row_height, 4);
        let window = Signal::derive_local(move || seen.window());
        let has_marquee =
            self.read_untracked(|state| state.picker.is_none() && state.allows_multiple());
        let blank_browser = self.clone();

        let focus_visible = self.0.list_focus_visible;
        let target = AnyView::new(view! {
            ListSurface(
                scroll_ref = list_node,
                class = "gds-files-list-surface"
            ) {
                PageTableArea(class = "gds-files-table-area") {
                    scroll(
                        node_ref = list_node,
                        class = "gds-files-grid-scroll",
                        attr:data-focus-visible = move || {
                            focus_visible.get().then(|| "true".to_owned())
                        },
                        on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                            if !has_marquee
                                && ev.button == Some(PointerButton::Primary)
                                && !ev.modifiers.shift()
                                && !ev.modifiers.primary()
                                && !super::drag::take_row_pointer_down(&blank_browser)
                            {
                                blank_browser.clear_blank_selection();
                            }
                        }
                    ) {
                        column(
                            class = "gds-files-grid-pane",
                            style:padding-left = format!("{GRID_PADDING}px"),
                            style:padding-right = format!("{GRID_PADDING}px"),
                            style:padding-top = move || {
                                Some(format!("{:.1}px", window.try_get().map_or(0.0, |w| w.lead)))
                            },
                            style:padding-bottom = move || {
                                Some(format!("{:.1}px", window.try_get().map_or(0.0, |w| w.trail)))
                            }
                        ) {
                            for grid_row in move || window.try_get().map(|w| w.indices()).unwrap_or_default(),
                                key = |grid_row: &usize| *grid_row
                            {
                                {{
                                    let browser = rows_browser.clone();
                                    let row_motion = motion_rows.clone();
                                    move || {
                                        let total = browser.read_structure(|state| state.tab().rows.len());
                                        let grid = row_motion.target.get();
                                        let columns = grid.columns.max(1);
                                        let start = grid_row * columns;
                                        let end = ((grid_row + 1) * columns).min(total);
                                        let tiles = (start..end)
                                            .map(|index| browser.visible_row(index, Some(row_motion.clone())))
                                            .collect::<Vec<_>>();
                                        view! {
                                            row(
                                                class = "gds-files-grid-row",
                                                style:height = format!("{:.1}px", grid.row_height),
                                                style:gap = format!("{GRID_GAP}px")
                                            ) {{tiles}}
                                        }
                                    }
                                }}
                            }
                        }
                    }
                }
            }
        });

        let dropped = self.background_drop_target(target);
        let surface = if has_marquee {
            self.marquee_surface(dropped, marquee::Layout::Grid)
        } else {
            dropped
        };
        self.directory_menu(surface)
    }

    fn visible_row(&self, index: usize, motion: Option<GridMotion>) -> AnyView {
        let is_grid = motion.is_some();
        let browser = self.clone();
        let row_key = local_memo(move || {
            let revision = browser.0.revision.get();
            let state = browser.0.state.borrow();
            let tab = state.tab();
            let entry = tab.rows.get(index).map(Row::key);
            (tab.id, entry, tab.columns.clone(), revision)
        });
        let render_browser = self.clone();
        AnyView::new(view! {
            box(
                class = "gds-files-row-wrap",
                attr:data-grid = is_grid.then(|| "true".to_owned())
            ) {
                {{move || {
                    row_key.with(|_| ());
                    let _z = enter_non_reactive_zone();
                    render_browser.row_view(index, motion.clone())
                }}}
            }
        })
    }

    fn row_view(&self, index: usize, motion: Option<GridMotion>) -> AnyView {
        let (row, columns, grid, cut, hidden) = self.read_structure(|state| {
            let tab = state.tab();
            let Some(row) = tab.rows.get(index).cloned() else {
                return (
                    None,
                    tab.columns.clone(),
                    tab.mode == ViewMode::Grid,
                    None,
                    false,
                );
            };
            let cut = row.path().map(|path| path.to_owned());
            let hidden = match &row {
                Row::File(entry) => entry.is_hidden,
                Row::Remote(entry) => entry.name.starts_with('.'),
                _ => false,
            };
            (
                Some(row),
                tab.columns.clone(),
                tab.mode == ViewMode::Grid,
                cut,
                hidden,
            )
        });
        let Some(row) = row else {
            return AnyView::new(view! { box() });
        };

        let key = row.key();
        let selected_sig = Signal::derive_local({
            let browser = self.clone();
            let key = key.clone();
            move || {
                let _ = browser.0.selection.get();
                let _ = browser.0.revision.get();
                browser
                    .0
                    .state
                    .borrow()
                    .tab()
                    .navigation
                    .view()
                    .selection
                    .contains(&key)
            }
        });
        let cursor_sig = Signal::derive_local({
            let browser = self.clone();
            let key = key.clone();
            move || {
                if !browser.0.list_focus_visible.get() {
                    return false;
                }
                let _ = browser.0.selection.get();
                let _ = browser.0.revision.get();
                browser
                    .0
                    .state
                    .borrow()
                    .tab()
                    .navigation
                    .view()
                    .cursor
                    .as_ref()
                    == Some(&key)
            }
        });
        let selection_goal =
            Signal::derive_local(move || if selected_sig.get() { 1.0_f32 } else { 0.0_f32 });
        let selection_progress = use_transition(selection_goal, WORKSPACE_DURATION_MS);

        let label = row.label();
        let a11y_path = row.path().map(|path| path.display().to_string());
        let desc_attrs = a11y_path.as_ref().map_or_else(Attrs::new, |path| {
            Attrs::new()
                .a11y_from(A11yBinding::unspecified().description(SharedString::from(path.clone())))
        });

        let cut_for_badge = cut.clone();
        let is_cut = Signal::derive_local(move || {
            let _ = Clipboard::current().snapshot();
            cut_for_badge
                .as_ref()
                .is_some_and(|path| Clipboard::current().is_cut(path))
        });
        let drag_browser = self.clone();
        let ghosted = Signal::derive_local(move || {
            let _ = Clipboard::current().snapshot();
            cut.as_ref().is_some_and(|path| {
                Clipboard::current().is_cut(path)
                    || drag_browser.is_drag_source(path)
                    || super::drag::FileDrag::is_source(path)
            })
        });

        let select_row = self.clone();
        let open_row = self.clone();
        let context_row = self.clone();

        if grid {
            let geometry = self.grid_geometry_untracked();
            let artwork = self.artwork(&row, geometry.icon_size, selection_progress);
            let a11y_label = SharedString::from(label.clone());
            let tile_tooltip = wrap_tokens(&label);
            let tile_pres = motion
                .as_ref()
                .map(|grid_motion| grid_motion.present_tile(key, index));
            let tile_width =
                tile_pres.map_or_else(|| Signal::stored_local(geometry.width), |p| p.width);
            let tile_height =
                tile_pres.map_or_else(|| Signal::stored_local(geometry.tile_height), |p| p.height);
            let tile_offset = tile_pres.map(|p| p.offset);
            let name_size = geometry.name_size;
            let line_height = geometry.line_height;
            let artwork_height = geometry.artwork_height;
            let name_height = geometry.name_height;

            let tile = AnyView::new(view! {
                column(
                    class = "gds-files-tile",
                    style:width = move || Some(format!("{:.1}px", tile_width.get())),
                    style:height = move || Some(format!("{:.1}px", tile_height.get())),
                    style:left = move || {
                        tile_offset.and_then(|off| {
                            let dx = off.get().0;
                            (dx.abs() > 0.05).then(|| format!("{dx:.1}px"))
                        })
                    },
                    style:top = move || {
                        tile_offset.and_then(|off| {
                            let dy = off.get().1;
                            (dy.abs() > 0.05).then(|| format!("{dy:.1}px"))
                        })
                    },
                    style:padding = format!("{GRID_PADDING}px"),
                    style:gap = format!("{GRID_GAP}px"),
                    attr:data-selected = move || selected_sig.get().then(|| "true".to_owned()),
                    attr:data-cursor = move || cursor_sig.get().then(|| "true".to_owned()),
                    attr:data-ghosted = move || ghosted.get().then(|| "true".to_owned()),
                    attr:data-hidden-entry = hidden.then(|| "true".to_owned()),
                    a11y:role = Role::ListItem,
                    a11y:label = a11y_label,
                    a11y:selected = move || selected_sig.get(),
                    {..desc_attrs},
                    on:click = move |ev: &mut EventCx<'_, events::Click>| {
                        select_row.select_row(index, ev.modifiers);
                        select_row.focus_content();
                    },
                    on:double_click = move |_: &mut EventCx<'_, events::DoubleClick>| {
                        open_row.open_row(index, false);
                    },
                    on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                        if ev.button == Some(PointerButton::Secondary) {
                            context_row.select_context(index);
                        }
                    }
                ) {
                    Show(when = move || is_cut.get()) {
                        box(class = "gds-files-tile-cut", a11y:hidden = true) {
                            Icon(name = IconName::Scissors, size = 12.0)
                        }
                    }
                    box(
                        class = "gds-files-tile-artwork",
                        style:height = format!("{artwork_height:.1}px")
                    ) {
                        {artwork}
                    }
                    box(
                        class = "gds-files-tile-name-wrap",
                        style:height = format!("{name_height:.1}px")
                    ) {
                        text(
                            class = "gds-files-tile-name",
                            style:font-size = format!("{name_size:.1}px"),
                            style:line-height = format!("{line_height:.1}px"),
                            style:max-height = format!("{:.1}px", line_height * 2.0)
                        ) {{label}}
                    }
                }
            });

            let tipped = AnyView::new(view! {
                HintTooltip(text = tile_tooltip) { {tile} }
            });
            let dragged = self.file_row_drag(tipped, &row, Some(geometry));
            return self.row_menu(dragged, index);
        }

        // Table row (`ViewMode::List` and collection tables)
        let selected_binding = Binding::controlled(selected_sig, |_| {});
        let cells: Vec<AnyView> = if let Some(collection_cells) =
            self.collection_cells(&row, selected_binding)
        {
            collection_cells
        } else {
            let name_width_browser = self.clone();
            let label_tooltip = wrap_tokens(&label);
            let a11y_value = SharedString::from(label.clone());
            let select_label = self.clone();
            let open_label = self.clone();
            let label_view = AnyView::new(view! {
                row(class = "gds-files-row-name-group") {
                    text(
                        class = "gds-files-row-name-text",
                        a11y:role = Role::Label,
                        a11y:value = a11y_value,
                        a11y:selected = move || selected_sig.get(),
                        {..desc_attrs},
                        on:click = move |ev: &mut EventCx<'_, events::Click>| {
                            ev.stop_propagation();
                            select_label.select_row(index, ev.modifiers);
                            select_label.focus_content();
                        },
                        on:double_click = move |ev: &mut EventCx<'_, events::DoubleClick>| {
                            ev.stop_propagation();
                            open_label.open_row(index, false);
                        }
                    ) {{label}}
                    Show(when = move || is_cut.get()) {
                        Icon(
                            name = IconName::Scissors,
                            size = 12.0,
                            class = "gds-files-cut-icon"
                        )
                    }
                }
            });

            let row_artwork = self.row_artwork_with_selection(&row, 18.0, selection_progress);
            let mut name = Some(AnyView::new(view! {
                HintTooltip(text = label_tooltip) {
                    row(
                        class = "gds-files-name-cell",
                        style:width = move || {
                            Some(format!("{:.1}px", name_width_browser.column_width(Column::Name)))
                        }
                    ) {
                        {row_artwork}
                        {label_view}
                    }
                }
            }));
            columns
                .columns()
                .iter()
                .map(|column| {
                    if *column == Column::Name {
                        name.take().unwrap_or_else(|| AnyView::new(view! { box() }))
                    } else {
                        self.cell(&row, *column)
                    }
                })
                .collect()
        };

        let target = AnyView::new(view! {
            row(
                class = "gds-files-row-content",
                attr:data-selected = move || selected_sig.get().then(|| "true".to_owned()),
                attr:data-cursor = move || cursor_sig.get().then(|| "true".to_owned()),
                attr:data-ghosted = move || ghosted.get().then(|| "true".to_owned()),
                attr:data-hidden-entry = hidden.then(|| "true".to_owned()),
                on:click = move |ev: &mut EventCx<'_, events::Click>| {
                    select_row.select_row(index, ev.modifiers);
                    select_row.focus_content();
                },
                on:double_click = move |_: &mut EventCx<'_, events::DoubleClick>| {
                    open_row.open_row(index, false);
                },
                on:pointer_down = move |ev: &mut EventCx<'_, events::PointerDown>| {
                    if ev.button == Some(PointerButton::Secondary) {
                        context_row.select_context(index);
                    }
                }
            ) {{cells}}
        });

        let dragged = self.file_row_drag(target, &row, None);
        self.row_menu(dragged, index)
    }

    fn cell(&self, row: &Row, column: Column) -> AnyView {
        let current_owner = matches!(
            (row, column),
            (Row::File(entry), Column::Owner) if EntryPresentation::owner(entry).current
        );
        let align_end = column == Column::Size;
        let width_browser = self.clone();

        let text_cell = |value: String| {
            let hint = wrap_tokens(&value);
            AnyView::new(view! {
                HintTooltip(text = hint) {
                    text(class = "gds-files-meta-text") {{value}}
                }
            })
        };

        let content = match (row, column) {
            (Row::File(entry), Column::Modified) => AnyView::new(view! {
                TimestampView(value = Signal::stored_local(entry.modified))
            }),
            (Row::File(entry), Column::Accessed) => AnyView::new(view! {
                TimestampView(value = Signal::stored_local(entry.details.accessed))
            }),
            (Row::Trash(entry), Column::Modified) => AnyView::new(view! {
                TimestampView(value = Signal::stored_local(entry.deleted_at))
            }),
            (Row::File(entry), Column::Name) => text_cell(entry.display_name.clone()),
            (Row::File(entry), Column::Type) => text_cell(EntryPresentation::type_label(entry)),
            (Row::File(entry), Column::DetailedType) => {
                text_cell(EntryPresentation::detailed_type(entry))
            }
            (Row::File(entry), Column::Size) => text_cell(EntryPresentation::size_label(entry)),
            (Row::File(entry), Column::Owner) => text_cell(EntryPresentation::owner(entry).text),
            (Row::File(entry), Column::Group) => text_cell(EntryPresentation::group(entry)),
            (Row::File(entry), Column::Permissions) => {
                entry.details.permissions.as_ref().map_or_else(
                    || text_cell("—".to_owned()),
                    |permissions| {
                        let hint = permissions.octal();
                        let sym = permissions.symbols();
                        AnyView::new(view! {
                            HintTooltip(text = hint) {
                                text(class = "gds-files-meta-text") {{sym}}
                            }
                        })
                    },
                )
            }
            _ => text_cell(String::new()),
        };

        AnyView::new(view! {
            box(
                class = "gds-files-meta-cell",
                attr:data-current-owner = current_owner.then(|| "true".to_owned()),
                attr:data-align-end = align_end.then(|| "true".to_owned()),
                style:width = move || Some(format!("{:.1}px", width_browser.column_width(column)))
            ) {{content}}
        })
    }

    fn page_actions(&self) -> AnyView {
        if self.compact() {
            return self.page_menu();
        }
        if let Some(actions) = self.special_page_actions() {
            return actions;
        }
        let grid = self.read(|state| state.tab().mode == ViewMode::Grid);
        let favorite =
            self.read(|state| state.places.is_favorite(state.tab().navigation.location()));
        let hidden = self.0.preferences.state().with(|p| p.files.show_hidden);
        let details = self.0.details.get();
        let details_button = self.details_available().then(|| {
            self.control(
                IconName::Info,
                "details-toggle",
                Browser::toggle_details,
                Some(Command::Details),
                Some(Signal::stored_local(details)),
            )
        });

        AnyView::new(view! {
            row(class = "gds-files-page-action-strip") {
                {{self.control(
                    if grid { IconName::List } else { IconName::LayoutGrid },
                    "view-toggle",
                    Browser::toggle_view,
                    Some(Command::ViewMode),
                    None,
                )}}
                {{self.control(
                    IconName::Star,
                    "favorite-toggle",
                    Browser::favorite,
                    Some(Command::Favorite),
                    Some(Signal::stored_local(favorite)),
                )}}
                {{self.column_menu()}}
                {{self.control(
                    IconName::Eye,
                    "show-hidden",
                    |browser| browser.edit_preferences(|preferences| {
                        preferences.files.show_hidden = !preferences.files.show_hidden;
                    }),
                    Some(Command::Hidden),
                    Some(Signal::stored_local(hidden)),
                )}}
                {details_button}
                {{self.control(
                    IconName::RefreshCw,
                    "refresh",
                    Browser::refresh,
                    Some(Command::Refresh),
                    None,
                )}}
            }
        })
    }

    fn status(&self) -> String {
        let visible_volumes = self
            .0
            .volumes
            .snapshot()
            .volumes
            .iter()
            .filter(|volume| volume.visible)
            .count();
        let filter = self.0.filter.get();
        self.read(|state| {
            let tab = state.tab();
            if tab.navigation.is_loading() || matches!(tab.contents, Contents::LoadingCollection) {
                return i18n::text("loading");
            }
            if matches!(tab.navigation.location(), Location::Volumes)
                || matches!(tab.contents, Contents::Volumes)
            {
                let count = if matches!(tab.contents, Contents::Volumes) && !filter.is_empty() {
                    tab.rows.len()
                } else {
                    visible_volumes
                };
                return i18n::count("volume-count", count);
            }
            if !filter.is_empty() {
                let total = match &tab.contents {
                    Contents::Directory => tab.navigation.directory().map(|dir| dir.entries.len()),
                    Contents::Places(items) => Some(items.len()),
                    Contents::Remote { listing, .. } => Some(listing.entries.len()),
                    Contents::Tags { entries, .. } if !tab.is_pure_tag_catalogue() => {
                        Some(entries.len())
                    }
                    _ => None,
                };
                if let Some(total) = total {
                    return gds::t!(
                        "filter-count",
                        visible = tab.rows.len() as i64,
                        total = total as i64
                    );
                }
            }
            i18n::count("item-count", tab.rows.len())
        })
    }

    fn prompt_view(
        &self,
        prompt: Prompt,
        options: ConfirmOptions,
        generation: u64,
        decision_slot: &PromptDecisionHandler,
    ) -> AnyView {
        *decision_slot.borrow_mut() = None;
        match prompt {
            Prompt::Closed => AnyView::new(view! { box() }),
            Prompt::About => AnyView::new(view! {
                box(
                    class = "gds-files-about-box",
                    style:width = format!("{ABOUT_WIDTH}px"),
                    style:height = format!("{ABOUT_HEIGHT}px")
                ) {
                    About(application = Application::cargo("gds-files"))
                }
            }),
            Prompt::Confirm {
                title,
                message,
                primary,
                accept,
            } => {
                let confirm_browser = self.downgrade();
                let settled = Rc::new(Cell::new(false));
                let decision_cb = UnsyncCallback::new(move |decision: DialogDecision| {
                    if settled.get() {
                        return;
                    }
                    let Some(browser) = confirm_browser.upgrade() else {
                        return;
                    };
                    if browser.0.prompt_generation.get_untracked() != generation
                        || matches!(browser.0.prompt.get_untracked(), Prompt::Closed)
                    {
                        return;
                    }
                    settled.set(true);
                    match decision {
                        DialogDecision::Confirm => {
                            browser.0.prompt.set(Prompt::Closed);
                            accept();
                        }
                        DialogDecision::Cancel | DialogDecision::Dismiss => {
                            browser.cancel_prompt();
                        }
                    }
                });

                *decision_slot.borrow_mut() = Some((Rc::new(Cell::new(false)), decision_cb));
                {
                    let cleanup_slot = Rc::clone(decision_slot);
                    on_cleanup_local(move || {
                        *cleanup_slot.borrow_mut() = None;
                    });
                }

                let shown_items: Vec<String> = options
                    .items
                    .iter()
                    .take(CONFIRM_PREVIEW_LIMIT)
                    .cloned()
                    .collect();
                let extra_from_slice = options.items.len().saturating_sub(CONFIRM_PREVIEW_LIMIT);
                let remaining = options.overflow_count.saturating_add(extra_from_slice);
                let header_icon = options.icon;
                let destructive = options.destructive;
                let primary_icon = options.primary_icon.unwrap_or(IconName::Check);

                AnyView::new(view! {
                    column(class = "gds-files-confirm-body") {
                        DialogHeader {
                            {if let Some(icon) = header_icon {
                                AnyView::new(view! {
                                    row(class = "gds-files-confirm-title-row") {
                                        Icon(
                                            name = icon,
                                            size = 18.0,
                                            class = "gds-files-confirm-title-icon",
                                            attr:data-destructive = destructive.then(|| "true".to_owned()),
                                        )
                                        DialogTitle {{title}}
                                    }
                                })
                            } else {
                                AnyView::new(view! {
                                    DialogTitle {{title}}
                                })
                            }}
                            DialogDescription {{message}}
                        }
                        {(!shown_items.is_empty()).then(|| {
                            let rows = shown_items
                                .into_iter()
                                .map(|item| {
                                    view! {
                                        text(class = "gds-files-confirm-item") {{item}}
                                    }
                                    .into_view()
                                })
                                .collect::<Vec<_>>();
                            let more = (remaining > 0).then(|| {
                                let count_label = i18n::count("operation-more-items", remaining);
                                view! {
                                    text(class = "gds-files-confirm-more") {{count_label}}
                                }
                            });
                            view! {
                                column(class = "gds-files-confirm-items") {
                                    {rows}
                                    {more}
                                }
                            }
                        })}
                        DialogFooter {
                            DialogActions(
                                primary_label = primary,
                                primary_icon = primary_icon,
                                cancel_label = i18n::text("operation-cancel"),
                                destructive = destructive,
                                on_decision = decision_cb,
                            )
                        }
                    }
                })
            }
            Prompt::Name { form, submit, .. } => form.view(self, generation, submit, decision_slot),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CONFIRM_PREVIEW_LIMIT, WORKSPACE_CSS};

    #[test]
    fn workspace_css_drops_resting_table_border_and_toolbar_spacer_and_prioritizes_ghosted_opacity()
    {
        assert!(
            !WORKSPACE_CSS.contains(".gds-files-table-surface"),
            "data table must remain borderless at rest"
        );
        assert!(
            !WORKSPACE_CSS.contains(".gds-files-toolbar-spacer"),
            "toolbar spacer must be removed so address bar stretches to WindowControls"
        );
        assert!(
            super::super::chrome::CHROME_CSS.contains("--zgui-scrollbar-thumb: transparent"),
            "breadcrumb track must suppress horizontal scrollbar chrome via zgui scrollbar custom properties"
        );
        assert!(
            !super::super::chrome::CHROME_CSS.contains("scrollbar-width"),
            "chrome stylesheet must not emit unsupported scrollbar-width CSS declarations"
        );

        let row_hidden = WORKSPACE_CSS
            .find(".gds-files-row-content[data-hidden-entry=\"true\"]")
            .expect("row hidden rule");
        let row_ghosted = WORKSPACE_CSS
            .find(".gds-files-row-content[data-ghosted=\"true\"]")
            .expect("row ghosted rule");
        assert!(
            row_ghosted > row_hidden,
            "ghosted row rule (0.35) must follow hidden rule (0.65) so cut/dragged dotfiles render at 35%"
        );

        let tile_hidden = WORKSPACE_CSS
            .find(".gds-files-tile[data-hidden-entry=\"true\"]")
            .expect("tile hidden rule");
        let tile_ghosted = WORKSPACE_CSS
            .find(".gds-files-tile[data-ghosted=\"true\"]")
            .expect("tile ghosted rule");
        assert!(
            tile_ghosted > tile_hidden,
            "ghosted tile rule (0.35) must follow hidden rule (0.65) so cut/dragged dotfiles render at 35%"
        );
    }

    #[test]
    fn confirm_preview_caps_at_five_items_and_accumulates_overflow() {
        let items: Vec<String> = (1..=8).map(|i| format!("item-{i}.txt")).collect();
        let shown: Vec<String> = items.iter().take(CONFIRM_PREVIEW_LIMIT).cloned().collect();
        let extra = items.len().saturating_sub(CONFIRM_PREVIEW_LIMIT);
        assert_eq!(shown.len(), 5);
        assert_eq!(extra, 3);
    }
}
