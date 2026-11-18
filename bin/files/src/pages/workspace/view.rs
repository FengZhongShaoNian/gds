use gds::appearance::tokens::DataListStyle;
use gds::i18n;
use gds::ui::about::{AboutProps, Application, HEIGHT as ABOUT_HEIGHT, WIDTH as ABOUT_WIDTH};
use gds::ui::button::IconButtonProps;
use gds::ui::feedback::dialog::{
    DialogActionsProps, DialogContentProps, DialogDecision, DialogDescriptionProps,
    DialogFooterProps, DialogHeaderProps, DialogProps, DialogTitleProps,
};
use gds::ui::feedback::notifications::{NotificationHostProps, error as notify_error};
use gds::ui::feedback::tooltip::{HintTooltipProps, wrap_tokens};
use gds::ui::frame::{
    FooterProps, FrameProps, ToolbarProps, WindowDragAreaProps, WorkspaceFrameProps,
};
use gds::ui::interaction::keytips::{KeytipHostProps, register_keytip};
use gds::ui::page::{
    ContentPageProps, PageActionsProps, PageContentProps, PageHeaderProps, PageTableAreaProps,
    PageTitleProps,
};
use gds::ui::time::TimestampViewProps;
use gds::ui::window::WindowControlsProps;
use gds::ui::{IconName, IconProps};
use zgui::geom::{CssPx, Point};
use zgui::prelude::*;
use zgui::reactive::{
    Owner, RenderEffect, RwSignal, UnsyncCallback, enter_non_reactive_zone, on_cleanup_local,
};
use zgui::view::{Scoped, current_host, events, focus_is_text_entry, install_stylesheet};
use zgui::vocab::{PointerButton, Role, SharedString};
use zgui::{css, view};
use zgui_ui::{ButtonProps, ButtonVariant, SpinnerProps, VirtualListProps, Virtualize};
use zgui_ui_primitives::Binding;

use super::geometry::{GRID_GAP, GRID_PADDING, GridMotion};
use super::marquee;
use super::menus::Surface;
use super::model::{Contents, Operation, Pane, Prompt, Row};
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

    .gds-files-menu-controls {
        display: flex;
        align-items: center;
        justify-content: center;
        flex: none;
    }

    .gds-files-toolbar {
        display: flex;
        flex: 1 1 0%;
        align-items: center;
        gap: 6px;
        min-width: 0px;
    }

    .gds-files-toolbar-files {
        display: flex;
        flex: 1 1 0%;
        align-items: center;
        gap: 6px;
        min-width: 0px;
    }

    .gds-files-toolbar-spacer {
        flex: 0 1 32px;
        min-width: 12px;
        height: 32px;
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

    .gds-files-nav-group {
        display: flex;
        flex: none;
        align-items: center;
        gap: 2px;
    }

    .gds-files-address-bar {
        display: flex;
        flex: 1 1 0%;
        align-items: center;
        min-width: 120px;
        height: 34px;
        border-radius: var(--gds-radius-control, 6px);
        border: 1px solid var(--gds-border);
        background-color: var(--gds-panel);
        overflow: hidden;
    }

    .gds-files-address-bar[data-editing="true"] {
        border-color: var(--gds-outline);
    }

    .gds-files-address-input {
        flex: 1 1 0%;
        min-width: 0px;
        height: 100%;
        border-width: 0px;
        background-color: transparent;
    }

    .gds-files-address-loading {
        display: flex;
        flex: 1 1 0%;
        align-items: center;
        min-width: 0px;
        padding: 0px 8px;
    }

    .gds-files-address-loading-text {
        width: 100%;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--gds-muted-foreground);
    }

    .gds-files-address-read {
        display: flex;
        flex: 1 1 0%;
        align-items: center;
        min-width: 0px;
        padding-right: 4px;
    }

    .gds-files-breadcrumbs {
        flex: 1 1 0%;
        min-width: 0px;
        height: 100%;
        overflow-x: auto;
        overflow-y: hidden;
    }

    .gds-files-breadcrumbs-track {
        display: flex;
        align-items: center;
        height: 100%;
        gap: 2px;
        padding: 0px 4px;
    }

    .gds-files-crumb {
        display: flex;
        flex: none;
        align-items: center;
        height: 28px;
        padding: 0px 6px;
        border-radius: 4px;
        cursor: pointer;
        color: var(--gds-muted-foreground);
    }

    .gds-files-crumb[data-current="true"] {
        color: var(--gds-foreground);
        font-weight: 500;
    }

    .gds-files-crumb:hover {
        background-color: var(--gds-hover);
    }

    .gds-files-crumb-text {
        white-space: nowrap;
    }

    .gds-files-filter {
        display: flex;
        align-items: center;
        width: 210px;
        min-width: 110px;
        height: 30px;
        gap: 4px;
        padding: 0px 6px;
        border-radius: var(--gds-radius-control, 6px);
        border: 1px solid var(--gds-border);
        background-color: var(--gds-panel);
    }

    .gds-files-filter-icon {
        flex: none;
        color: var(--gds-muted-foreground);
    }

    .gds-files-filter-input {
        flex: 1 1 0%;
        min-width: 0px;
        height: 100%;
        border-width: 0px;
        background-color: transparent;
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

    .gds-files-sidebar-tabs-section {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-sidebar-tabs-section[data-fixed-height="true"] {
        flex: 0 0 auto;
    }

    .gds-files-sidebar-volumes-section {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-sidebar-volumes-section[data-section-collapsed="true"] {
        flex: 0 0 auto;
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

    .gds-files-sidebar-scroll {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow-y: auto;
        overflow-x: hidden;
        gap: 4px;
    }

    .gds-files-sidebar-locations {
        display: flex;
        flex-direction: column;
        width: 100%;
        min-width: 0px;
        overflow-x: hidden;
        gap: 4px;
    }

    .gds-files-sidebar-row-wrap {
        display: flex;
        flex-direction: column;
        width: 100%;
        max-width: 100%;
        min-width: 0px;
        overflow: hidden;
    }

    .gds-files-sidebar-row-wrap .zui-tooltip__trigger {
        display: flex;
        width: 100%;
        max-width: 100%;
        min-width: 0px;
        overflow: hidden;
    }

    .gds-files-sidebar-heading {
        display: flex;
        flex: 1 1 0%;
        align-items: center;
        gap: 6px;
        min-width: 0px;
        height: 100%;
        padding: 0px 6px;
        border-radius: 4px;
        cursor: pointer;
    }

    .gds-files-sidebar-heading:hover {
        background-color: var(--gds-hover);
    }

    .gds-files-sidebar-chevron {
        flex: none;
        transform: rotate(90deg);
        transition: transform var(--gds-motion-fast, 100ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }

    .gds-files-sidebar-heading[data-collapsed="true"] .gds-files-sidebar-chevron {
        transform: rotate(0deg);
    }

    .gds-files-sidebar-splitter {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 9px;
        flex: none;
        cursor: ns-resize;
    }

    .gds-files-sidebar-splitter-line {
        width: 100%;
        height: 1px;
        background-color: var(--gds-border);
    }

    .gds-files-sidebar-splitter:hover .gds-files-sidebar-splitter-line {
        background-color: var(--gds-primary);
    }

    .gds-files-sidebar-empty {
        padding: 6px;
        font-size: 12px;
        color: var(--gds-muted-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-tab-item {
        width: 100%;
        max-width: 100%;
        min-width: 0px;
        overflow: hidden;
        height: 44px;
        gap: 10px;
        border-radius: 6px;
    }

    .gds-files-location-item {
        width: 100%;
        max-width: 100%;
        min-width: 0px;
        overflow: hidden;
        gap: 10px;
        border-radius: 6px;
    }

    .gds-files-volume-item {
        padding: 6px 10px;
        border-radius: 6px;
    }

    .gds-files-volume-inner {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        gap: 4px;
    }

    .gds-files-volume-header {
        display: flex;
        align-items: center;
        gap: 10px;
        min-width: 0px;
    }

    .gds-files-volume-unmounted {
        font-size: 12px;
        color: var(--gds-muted-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-volume-capacity {
        min-width: 0px;
        overflow: hidden;
    }

    .gds-capacity__fill {
        width: var(--gds-capacity-width, 0%);
    }

    .gds-files-nav-icon {
        flex: none;
    }

    .gds-files-workspace-column {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }

    .gds-files-workspace-frame {
        display: flex;
        flex: 1 1 0%;
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

    .gds-files-content-focus {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
        outline: 1px solid transparent;
        outline-offset: -1px;
    }

    .gds-files-content-focus[data-focus-visible="true"] {
        outline-color: var(--gds-outline);
    }

    .gds-files-details-pane {
        display: flex;
        flex-direction: column;
        flex: none;
        width: 280px;
        min-height: 0px;
        border-left: 1px solid var(--gds-border);
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

    .gds-files-table-area {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
    }

    .gds-files-table-x {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        min-width: 0px;
        min-height: 0px;
        overflow-x: auto;
        overflow-y: hidden;
    }

    .gds-files-table-inner {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
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
        min-width: 0px;
        height: 100%;
        padding: 0px 8px 0px 12px;
        cursor: pointer;
    }

    .gds-files-header-button[data-name-col="true"] {
        padding-left: 40px;
    }

    .gds-files-header-button:hover {
        background-color: var(--gds-hover);
    }

    .gds-files-header-label {
        flex: 1 1 0%;
        min-width: 0px;
        font-size: 11px;
        color: var(--gds-muted-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-header-sort {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        width: 12px;
        height: 18px;
        color: var(--gds-muted-foreground);
    }

    .gds-files-header-sort[data-active="true"] {
        color: var(--gds-foreground);
    }

    .gds-files-header-resize {
        width: 5px;
        height: 100%;
        flex: none;
        cursor: ew-resize;
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
    }

    .gds-files-row-content {
        display: flex;
        align-items: center;
        width: 100%;
        height: 100%;
        min-width: 0px;
        border-radius: 6px;
        user-select: none;
        cursor: pointer;
    }

    .gds-files-row-content:hover {
        background-color: var(--gds-hover);
    }

    .gds-files-row-content[data-selected="true"] {
        background-color: var(--gds-selection);
        color: var(--gds-selection-foreground);
    }

    .gds-files-row-content[data-ghosted="true"] {
        opacity: 0.35;
    }

    .gds-files-row-content[data-hidden-entry="true"] {
        opacity: 0.65;
    }

    .gds-files-name-cell {
        display: flex;
        flex: none;
        align-items: center;
        height: 100%;
        gap: 10px;
        padding: 0px 12px;
        min-width: 0px;
    }

    .gds-files-row-name-text {
        flex: 1 1 0%;
        min-width: 0px;
        font-size: 13px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-meta-cell {
        display: flex;
        flex: none;
        align-items: center;
        min-width: 0px;
        padding: 0px 12px;
        font-size: 12px;
        color: var(--gds-muted-foreground);
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
        padding-left: 20px;
    }

    .gds-files-grid-row {
        display: flex;
        align-items: center;
        gap: 8px;
    }

    .gds-files-tile {
        display: flex;
        flex-direction: column;
        align-items: center;
        padding: 8px;
        gap: 8px;
        border-radius: 8px;
        border: 1px solid transparent;
        user-select: none;
        cursor: pointer;
    }

    .gds-files-tile:hover {
        background-color: var(--gds-hover);
    }

    .gds-files-tile[data-selected="true"] {
        background-color: var(--gds-selection);
        color: var(--gds-selection-foreground);
        border-color: var(--gds-primary);
    }

    .gds-files-tile[data-ghosted="true"] {
        opacity: 0.35;
    }

    .gds-files-tile[data-hidden-entry="true"] {
        opacity: 0.65;
    }

    .gds-files-tile-artwork {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: center;
        width: 100%;
    }

    .gds-files-tile-name-wrap {
        width: 100%;
        min-width: 0px;
        overflow: hidden;
        text-align: center;
    }

    .gds-files-tile-name {
        width: 100%;
        min-width: 0px;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .gds-files-directory-surface {
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

    .gds-files-footer-spacer {
        flex: 1 1 0%;
        min-width: 0px;
    }

    .gds-files-footer-controls {
        display: flex;
        flex: none;
        align-items: center;
        gap: 4px;
    }

    .gds-files-status-controls {
        display: flex;
        align-items: center;
        gap: 2px;
        min-width: 0px;
    }

    .gds-files-status-slot {
        display: flex;
        min-width: 0px;
    }

    .gds-files-status-button {
        display: flex;
        align-items: center;
        min-width: 0px;
        height: 28px;
        padding: 0px 6px;
        border-radius: 6px;
        cursor: pointer;
    }

    .gds-files-status-button:hover {
        background-color: var(--gds-hover);
    }

    .gds-files-status-button-inner {
        display: flex;
        align-items: center;
        gap: 6px;
        min-width: 0px;
    }

    .gds-files-status-icon-wrap {
        position: relative;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 24px;
        height: 24px;
        flex: none;
    }

    .gds-files-status-badge {
        position: absolute;
        top: 0px;
        right: 0px;
        min-width: 12px;
        height: 12px;
        border-radius: 999px;
        padding: 0px 2px;
        background-color: var(--gds-primary);
        color: var(--gds-selection-foreground);
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .gds-files-status-badge-text {
        font-size: 9px;
    }

    .gds-files-status-label {
        max-width: 240px;
        min-width: 0px;
        font-size: 12px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .gds-files-status-popover {
        width: 320px;
        max-height: 320px;
        padding: 4px;
    }

    .gds-files-status-empty {
        padding: 12px;
        font-size: 12px;
        color: var(--gds-muted-foreground);
    }

    .gds-files-status-list-wrap {
        width: 100%;
        min-width: 0px;
    }

    .gds-files-status-list {
        width: 100%;
        height: 100%;
    }

    .gds-files-status-row {
        display: flex;
        align-items: center;
        gap: 8px;
        min-width: 0px;
        height: 32px;
        padding: 0px 8px;
        border-radius: 4px;
    }

    .gds-files-status-row:hover {
        background-color: var(--gds-hover);
    }

    .gds-files-status-row-icon {
        flex: none;
    }

    .gds-files-status-row-label {
        flex: 1 1 0%;
        min-width: 0px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
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
    .gds-files-prompt-dialog[data-about="true"] {
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
    "#
);

pub(super) fn local_memo<T: PartialEq + Clone + 'static>(
    compute: impl Fn() -> T + 'static,
) -> Signal<T, LocalStorage> {
    let owner = Owner::current();
    let owner_fx = owner.clone();
    let compute = std::rc::Rc::new(compute);
    let compute_fx = std::rc::Rc::clone(&compute);
    let slot: std::rc::Rc<std::cell::OnceCell<RwSignal<T, LocalStorage>>> =
        std::rc::Rc::new(std::cell::OnceCell::new());
    let slot_fx = std::rc::Rc::clone(&slot);
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
    pub(super) const fn action_keytip(command: Option<Command>) -> Option<&'static str> {
        let Some(command) = command else {
            return None;
        };
        Some(match command {
            Command::Sidebar => "B",
            Command::Back => "AL",
            Command::Forward => "AR",
            Command::Parent => "U",
            Command::EditLocation => "E",
            Command::Terminal => "K",
            Command::ViewMode => "G",
            Command::Favorite => "D",
            Command::Hidden => "H",
            Command::Details => "P",
            Command::Refresh => "R",
            Command::Sort => "S",
            Command::Columns => "C",
            _ => return None,
        })
    }

    pub fn view(&self) -> impl IntoView + use<> {
        let browser = self.clone();
        Scoped::new(move || browser.view_scoped())
    }

    fn view_scoped(&self) -> impl IntoView + use<> {
        install_workspace_stylesheets();

        let keytip_active = RwSignal::new_local(false);
        let hint_context = self.clone();
        let keytip_guard_fx = RenderEffect::new(move |_| {
            let allow = hint_context.pane() == Pane::Files
                && matches!(hint_context.0.prompt.get(), Prompt::Closed)
                && matches!(hint_context.0.menus.surface.get(), Surface::Closed)
                && !hint_context.0.connector.is_connecting();
            if !allow && keytip_active.get_untracked() {
                keytip_active.set(false);
            }
        });
        on_cleanup_local(move || drop(keytip_guard_fx));

        if let Some(window) = self.0.window.clone() {
            let width_sig = self.0.window_width;
            let size = window.size();
            let scale = window.scale();
            let win_fx = RenderEffect::new(move |_| {
                let s = scale.get().max(1.0);
                width_sig.set((size.get().width.0 / s).max(1.0));
            });
            on_cleanup_local(move || drop(win_fx));
        } else {
            let root = self.0.root_focus;
            let width_sig = self.0.window_width;
            let root_size = root.observe_border_size();
            let root_fx = RenderEffect::new(move |_| {
                if let Some(sz) = root_size.get() {
                    let s = root.scale().max(1.0);
                    width_sig.set((sz.width.0 / s).max(1.0));
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
        let prompt_open = Signal::derive_local(move || !matches!(prompt_sig.get(), Prompt::Closed));

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
                                        attr:data-about = move || matches!(prompt_sig.get(), Prompt::About).then(|| "true".to_owned()),
                                        dismiss_on_outside_press = true,
                                        dismiss_on_escape = true
                                    ) {
                                        {{
                                            let pb = prompt_browser.clone();
                                            move || pb.prompt_view()
                                        }}
                                    }
                                }
                            }
                        }
                    }}
                }
            }
        }
    }

    fn help_view(&self) -> AnyView {
        let menu_controls = self.menu_controls();
        let root_ref = self.0.settings_node;
        AnyView::new(view! {
            Frame(class = "gds-files-frame") {
                Toolbar {
                    {menu_controls}
                    WindowDragArea(class = "gds-files-toolbar-title") {
                        text(class = "gds-files-toolbar-title-text") {{i18n::text("help")}}
                    }
                    WindowControls() {}
                }
                WorkspaceFrame(class = "gds-files-workspace-frame") {
                    box(
                        node_ref = root_ref,
                        tabindex = Focus::Sequential,
                        class = "gds-files-settings-host"
                    ) {
                        FilesHelp()
                    }
                }
                Footer { box() {} }
            }
        })
    }

    fn workspace_view(&self) -> AnyView {
        let browser = self.clone();
        AnyView::new(Scoped::new(move || browser.workspace_view_scoped()))
    }

    fn workspace_view_scoped(&self) -> AnyView {
        let expanded_memo = local_memo({
            let browser = self.clone();
            move || browser.sidebar_expanded()
        });
        let pane_memo = local_memo({
            let browser = self.clone();
            move || browser.pane()
        });

        let workspace_col = NodeRef::new();
        let workspace_size = workspace_col.observe_border_size();
        let resize_browser = self.clone();
        let term_size_fx = RenderEffect::new(move |_| {
            if let Some(sz) = workspace_size.get() {
                let s = workspace_col.scale().max(1.0);
                resize_browser.terminal_workspace_resized(sz.width.0 / s, sz.height.0 / s);
            }
        });
        on_cleanup_local(move || drop(term_size_fx));

        let sidebar_browser = self.clone();
        let content_browser = self.clone();
        let terminal_browser = self.clone();
        let status_browser = self.clone();

        let menu_controls = self.menu_controls();
        let toolbar = self.toolbar();
        let footer_controls = self.footer_controls();
        let status_controls = self.status_controls();

        AnyView::new(view! {
            Frame(class = "gds-files-frame") {
                Toolbar {
                    {menu_controls}
                    {toolbar}
                    WindowControls() {}
                }
                column(node_ref = workspace_col, class = "gds-files-workspace-column") {
                    WorkspaceFrame(class = "gds-files-workspace-frame") {
                        {{move || match pane_memo.get() {
                            Pane::Files => sidebar_browser.sidebar(gds::ui::sidebar::SidebarPresentation::default()),
                            Pane::Settings => {
                                let pres = gds::ui::sidebar::SidebarPresentation::from_collapsed(!expanded_memo.get());
                                sidebar_browser.0.settings.sidebar(pres)
                            }
                            Pane::Help => AnyView::new(view! { box(class = "gds-files-hidden") }),
                        }}}
                        {{move || match pane_memo.get() {
                            Pane::Files => content_browser.file_content(),
                            Pane::Settings => AnyView::new(view! {
                                box(
                                    node_ref = content_browser.0.settings_node,
                                    tabindex = Focus::Sequential,
                                    class = "gds-files-settings-host"
                                ) {
                                    {{content_browser.0.settings.content()}}
                                }
                            }),
                            Pane::Help => AnyView::new(view! { box(class = "gds-files-hidden") }),
                        }}}
                    }
                    {{move || {
                        let pres = gds::ui::sidebar::SidebarPresentation::from_collapsed(!expanded_memo.get());
                        terminal_browser.terminal_view(pres)
                    }}}
                }
                Footer {
                    row(class = "gds-files-footer-status") {
                        {{move || {
                            if pane_memo.get() == Pane::Files {
                                let sb = status_browser.clone();
                                AnyView::new(view! {
                                    text(class = "gds-files-footer-status-text") {{move || sb.status()}}
                                })
                            } else {
                                status_browser.0.settings.footer_text()
                            }
                        }}}
                        {status_controls}
                    }
                    box(class = "gds-files-footer-spacer") {}
                    {footer_controls}
                }
            }
        })
    }

    fn footer_controls(&self) -> AnyView {
        let terminal_sig = self.0.terminal;
        let click_browser = self.clone();
        let activate_browser = self.clone();
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
                        let act_b = activate_browser.clone();
                        let clk_b = click_browser.clone();
                        view! {
                            IconButton(
                                icon = IconName::Terminal,
                                label = terminal_label,
                                toggled = Some(terminal_toggled),
                                shortcut = Some(Command::Terminal.shortcut()),
                                keytip = Some("K"),
                                a11y:role = Role::Button,
                                on_activate = UnsyncCallback::new(move |()| act_b.toggle_terminal()),
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
        let activate_browser = self.clone();
        let shortcut = command.map(Command::shortcut);
        let keytip = Self::action_keytip(command);
        let active_sig = active.unwrap_or_else(|| Signal::stored_local(false));

        AnyView::new(view! {
            IconButton(
                icon = icon,
                label = Signal::derive_local(move || i18n::text(label)),
                active = active_sig,
                shortcut = shortcut,
                keytip = keytip,
                on_activate = UnsyncCallback::new(move |()| action(&activate_browser)),
                on:click = move |_| action(&click_browser)
            )
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
                width_sig.set((sz.width.0 / s).max(1.0));
            }
        });
        on_cleanup_local(move || drop(size_fx));

        register_keytip(
            "W",
            content_ref,
            UnsyncCallback::new({
                let b = self.clone();
                move |()| b.focus_content()
            }),
        );

        let body = local_memo({
            let browser = self.clone();
            move || browser.file_body()
        });

        let show_details = Signal::derive_local({
            let browser = self.clone();
            move || {
                !browser.compact()
                    && browser.0.details.get()
                    && !browser.read(|state| state.tab().is_pure_tag_catalogue())
            }
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

        let focus_out_ref = content_ref;
        let visible_sig = self.0.list_focus_visible;
        let filter_view = self.folder_filter();
        let picker_footer = self.picker_footer();

        AnyView::new(view! {
            row(class = "gds-files-content-shell") {
                ContentPage {
                    PageHeader {
                        PageTitle {
                            text(class = "gds-files-title-text") {{move || title_browser.read(|state| {
                                title_browser.location_title(state.tab().navigation.location())
                            })}}
                        }
                        PageActions(class = "gds-files-page-actions") {
                            {filter_view}
                            {move || actions_browser.page_actions()}
                        }
                    }
                    {move || notice_browser.collection_notice()}
                    PageContent {
                        box(
                            node_ref = content_ref,
                            class = "gds-files-content-focus",
                            tabindex = Focus::Sequential,
                            attr:data-focus-visible = move || visible_sig.get().then(|| "true".to_owned()),
                            on:focus_in = {
                                let b = focus_browser.clone();
                                move |ev: &mut EventCx<'_, events::FocusIn>| {
                                    b.0.list_focused.set(true);
                                    visible_sig.set(ev.cause.shows_ring());
                                }
                            },
                            on:focus_out = {
                                let b = focus_browser.clone();
                                move |_: &mut EventCx<'_, events::FocusOut>| {
                                    let stays_inside = current_host().is_some_and(|host| {
                                        host.focused().get_untracked().is_some_and(|active| {
                                            focus_out_ref.get_untracked() == Some(active)
                                                || focus_out_ref.contains(active)
                                        })
                                    });
                                    if !stays_inside {
                                        b.0.list_focused.set(false);
                                        visible_sig.set(false);
                                    }
                                }
                            },
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
                                let current = body.get();
                                let _z = enter_non_reactive_zone();
                                content_browser.render_file_body(current)
                            }}}
                        }
                        {picker_footer}
                    }
                }
                Show(when = move || show_details.get()) {
                    {{
                        let d_browser = details_browser.clone();
                        view! {
                            box(class = "gds-files-details-pane") {
                                {{d_browser.details_view()}}
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
        AnyView::new(view! {
            column(class = "gds-files-state-center") {
                text {{i18n::text(message)}}
            }
        })
    }

    fn column_width(&self, column: Column) -> f32 {
        let width = self
            .0
            .column_widths
            .with(|widths| widths.get(&column).copied());
        width.unwrap_or_else(|| {
            let viewport = self.table_viewport_width();
            let weights: f32 = self.read(|state| {
                state
                    .tab()
                    .columns
                    .columns()
                    .iter()
                    .map(|col| Self::column_weight(*col))
                    .sum()
            });
            viewport * Self::column_weight(column) / weights.max(1.0)
        })
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

    fn table_width(&self) -> f32 {
        let viewport = self.table_viewport_width();
        if let Some(columns) = self.collection_columns() {
            return columns
                .into_iter()
                .map(|(_, width)| width)
                .sum::<f32>()
                .max(viewport);
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
        .max(viewport)
    }

    fn column_header(&self, column: Column) -> AnyView {
        let width_browser = self.clone();
        let sort_browser = self.clone();
        let resize_drag = self.clone();
        let resize_cancel = self.clone();
        let resize_end = self.clone();
        let sortable = self.sortable_columns().contains(&column);
        let sorting = self.read(|state| state.tab().sort);
        let active = sorting.key == column;
        let ascending = sorting.direction == Direction::Ascending;

        let drag_state: RwSignal<Option<(f32, f32)>, LocalStorage> = RwSignal::new_local(None);
        let start_width = RwSignal::new_local(self.column_width(column));
        let window = try_use_window();

        AnyView::new(view! {
            row(
                class = "gds-files-header-cell",
                style:width = move || Some(format!("{:.1}px", width_browser.column_width(column)))
            ) {
                control(
                    class = "gds-files-header-button",
                    attr:data-name-col = (column == Column::Name).then(|| "true".to_owned()),
                    tabindex = if sortable { Focus::Sequential } else { Focus::Programmatic },
                    a11y:role = Role::Button,
                    a11y:label = SharedString::from(i18n::text(column.label_id())),
                    a11y:disabled = !sortable,
                    on:click = move |_| {
                        if sortable {
                            sort_browser.sort_by(column);
                        }
                    }
                ) {
                    text(class = "gds-files-header-label") {{i18n::text(column.label_id())}}
                    {{sortable.then(|| AnyView::new(view! {
                        column(
                            class = "gds-files-header-sort",
                            attr:data-active = active.then(|| "true".to_owned())
                        ) {
                            {{(!active || ascending).then(|| AnyView::new(view! { Icon(name = IconName::ChevronUp, size = 8.0) }))}}
                            {{(!active || !ascending).then(|| AnyView::new(view! { Icon(name = IconName::ChevronDown, size = 8.0) }))}}
                        }
                    }))}}
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
                        let win = window.clone();
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

        let row_count = Signal::derive_local(move || {
            count_browser.read_structure(|state| state.tab().rows.len())
        });
        let empty = local_memo({
            let browser = self.clone();
            move || browser.collection_empty_label()
        });

        let target = AnyView::new(view! {
            PageTableArea(class = "gds-files-table-area") {
                scroll(node_ref = viewport, class = "gds-files-table-x") {
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
        });

        self.directory_menu(self.marquee_surface(
            self.background_drop_target(target),
            marquee::Layout::List { viewport },
        ))
    }

    fn grid_view(&self) -> AnyView {
        let browser = self.clone();
        AnyView::new(Scoped::new(move || browser.grid_view_scoped()))
    }

    fn grid_view_scoped(&self) -> AnyView {
        self.attach_list_scroll_observer();

        let list_node = self.0.list;
        let motion = GridMotion::new(self);
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

        let target = AnyView::new(view! {
            PageTableArea(class = "gds-files-table-area") {
                scroll(node_ref = list_node, class = "gds-files-grid-scroll") {
                    column(
                        class = "gds-files-grid-pane",
                        style:padding-left = format!("{GRID_PADDING}px"),
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
                                move || {
                                    let total = browser.read_structure(|state| state.tab().rows.len());
                                    let grid = motion.target.get();
                                    let columns = grid.columns.max(1);
                                    let start = grid_row * columns;
                                    let end = ((grid_row + 1) * columns).min(total);
                                    let tiles = (start..end)
                                        .map(|index| browser.visible_row(index, Some(motion)))
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
        });

        self.directory_menu(
            self.marquee_surface(self.background_drop_target(target), marquee::Layout::Grid),
        )
    }

    fn visible_row(&self, index: usize, motion: Option<GridMotion>) -> AnyView {
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
            box(class = "gds-files-row-wrap") {
                {{move || {
                    row_key.with(|_| ());
                    let _z = enter_non_reactive_zone();
                    render_browser.row_view(index, motion)
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
            let hidden = matches!(&row, Row::File(entry) if entry.display_name.starts_with('.'));
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
        let selection_progress =
            Signal::derive_local(move || if selected_sig.get() { 1.0_f32 } else { 0.0_f32 });

        let label = row.label();
        let a11y_path = row.path().map(|path| path.display().to_string());
        let desc_attrs = a11y_path.as_ref().map_or_else(Attrs::new, |path| {
            Attrs::new()
                .a11y_from(A11yBinding::unspecified().description(SharedString::from(path.clone())))
        });

        let ghosted = Signal::derive_local(move || {
            let _ = Clipboard::current().snapshot();
            cut.as_ref().is_some_and(|path| {
                Clipboard::current().is_cut(path) || super::drag::FileDrag::is_source(path)
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
            let tile_width = Signal::derive_local(move || {
                motion.map_or(geometry.width, |m| m.target.get().width)
            });
            let tile_height = Signal::derive_local(move || {
                motion.map_or(geometry.tile_height, |m| m.target.get().tile_height)
            });
            let name_size = geometry.name_size;
            let line_height = geometry.line_height;
            let artwork_height = geometry.artwork_height;
            let name_height = geometry.name_height;

            let tile = AnyView::new(view! {
                column(
                    class = "gds-files-tile",
                    style:width = move || Some(format!("{:.1}px", tile_width.get())),
                    style:height = move || Some(format!("{:.1}px", tile_height.get())),
                    style:padding = format!("{GRID_PADDING}px"),
                    style:gap = format!("{GRID_GAP}px"),
                    attr:data-selected = move || selected_sig.get().then(|| "true".to_owned()),
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
                            style:line-height = format!("{line_height:.1}px")
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
        let name_width_browser = self.clone();
        let label_tooltip = wrap_tokens(&label);
        let a11y_value = SharedString::from(label.clone());
        let select_label = self.clone();
        let open_label = self.clone();
        let label_view = AnyView::new(view! {
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
        });

        let row_artwork = self.row_artwork_with_selection(&row, 18.0, selection_progress);
        let name_cell = AnyView::new(view! {
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
        });

        let selected_binding = Binding::controlled(selected_sig, |_| {});
        let cells: Vec<AnyView> =
            if let Some(metadata) = self.collection_cells(&row, selected_binding) {
                std::iter::once(name_cell).chain(metadata).collect()
            } else {
                let mut name = Some(name_cell);
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
                {{self.control(
                    IconName::PanelRight,
                    "details-toggle",
                    Browser::toggle_details,
                    Some(Command::Details),
                    Some(Signal::stored_local(details)),
                )}}
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
        self.read(|state| match &state.operation {
            Operation::Running { label, .. } => label.clone(),
            Operation::Idle => {
                let tab = state.tab();
                if matches!(tab.contents, Contents::Volumes) {
                    return i18n::count("volume-count", tab.rows.len());
                }
                if tab.navigation.is_loading() {
                    i18n::text("loading")
                } else {
                    i18n::count("item-count", tab.rows.len())
                }
            }
        })
    }

    fn prompt_view(&self) -> AnyView {
        let prompt = self.0.prompt.get();
        let cancel_browser = self.clone();
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
                let cancel_action = cancel_browser;
                AnyView::new(view! {
                    column(class = "gds-files-confirm-body") {
                        DialogHeader {
                            DialogTitle {{title}}
                            DialogDescription {{message}}
                        }
                        DialogFooter {
                            DialogActions(
                                confirm_label = primary,
                                cancel_label = i18n::text("dialog-cancel"),
                                on_decision = UnsyncCallback::new(move |decision: DialogDecision| {
                                    match decision {
                                        DialogDecision::Confirm => {
                                            if let Some(browser) = confirm_browser.upgrade() {
                                                browser.0.prompt.set(Prompt::Closed);
                                                accept();
                                            }
                                        }
                                        DialogDecision::Cancel | DialogDecision::Dismiss => {
                                            cancel_action.cancel_prompt();
                                        }
                                    }
                                })
                            )
                        }
                    }
                })
            }
            Prompt::Name { form, submit, .. } => form.view(self, submit),
        }
    }
}
