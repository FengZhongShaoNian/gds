//! Shared Stylo CSS rules for GDS UI controls and surfaces.
//!
//! Theme palette, typography, and motion custom properties (`--gds-*` and
//! `--zui-*`) are installed per window by
//! [`crate::app::appearance::Appearance::install_window_sheet`]. This module
//! installs only structural component classes and focus-ring rules once per
//! window document.

use zgui::view::install_stylesheet;

const BASE_SHEET: &str = "gds-ui-base";

const BASE_CSS: &str = r#"
.gds-icon {
    display: inline-block;
    flex-shrink: 0;
    width: var(--gds-icon-size, 16px);
    height: var(--gds-icon-size, 16px);
    color: inherit;
}

.gds-surface {
    background: var(--gds-surface);
    color: var(--gds-surface-foreground);
    border: 1px solid var(--gds-border);
    border-radius: var(--gds-radius-control, 6px);
}

.gds-card-surface {
    background: var(--gds-panel);
    color: var(--gds-panel-foreground);
    border: 1px solid var(--gds-border);
    border-radius: var(--gds-radius-control, 6px);
    padding: var(--gds-content-padding, 12px);
}

.gds-content-container {
    display: flex;
    flex-direction: column;
    flex: 1 1 0%;
    min-width: 0;
    min-height: 0;
    padding: var(--gds-content-padding, 12px);
}

.gds-frame {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    border-radius: var(--gds-radius-window, 10px);
    border: 1px solid var(--gds-border);
    overflow: hidden;
    color: var(--gds-foreground);
    font-family: var(--gds-font-sans);
    font-size: var(--gds-font-size, 14px);
}

.gds-frame-content {
    display: flex;
    flex-direction: column;
    flex: 1 1 0%;
    min-width: 0;
    min-height: 0;
    background: var(--gds-content-bg, var(--gds-content));
}

.gds-toolbar {
    display: flex;
    flex-direction: row;
    align-items: center;
    height: var(--gds-toolbar-height, 52px);
    flex-shrink: 0;
    padding: 0 8px;
    gap: 6px;
}

.gds-footer {
    display: flex;
    flex-direction: row;
    align-items: center;
    height: var(--gds-footer-height, 36px);
    flex-shrink: 0;
    padding: 0 12px;
    gap: 8px;
    border-top: 1px solid var(--gds-border);
}

.gds-sidebar {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--gds-sidebar-bg, var(--gds-sidebar));
    color: var(--gds-sidebar-foreground);
    border-right: 1px solid var(--gds-border);
}

.gds-nav-item {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    min-height: var(--gds-sidebar-item-height, 38px);
    padding: 0 8px;
    border-radius: var(--gds-radius-control, 6px);
    color: inherit;
    cursor: pointer;
}

.gds-nav-item:hover {
    background: var(--gds-sidebar-hover);
}

.gds-nav-item[data-selected="true"] {
    background: var(--gds-selection);
    color: var(--gds-selection-foreground);
}

.gds-nav-item[data-drop-target="true"] {
    background: var(--gds-drag-target);
    outline: 1px solid var(--gds-primary);
}

.gds-hover-slot {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: var(--gds-icon-button-size, 32px);
    height: var(--gds-icon-button-size, 32px);
    flex-shrink: 0;
}

.gds-hover-slot__action {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
}

.gds-hover-slot:hover .gds-hover-slot__action,
.gds-hover-slot[data-revealed="true"] .gds-hover-slot__action,
.gds-nav-item:hover .gds-hover-slot__action {
    opacity: 1;
}

.gds-hover-slot:hover .gds-hover-slot__status,
.gds-hover-slot[data-revealed="true"] .gds-hover-slot__status,
.gds-nav-item:hover .gds-hover-slot__status {
    opacity: 0;
}

.gds-corner-action {
    position: absolute;
    top: 1px;
    right: 1px;
    opacity: 0;
}

.gds-corner-action[data-revealed="true"],
.gds-nav-item:hover .gds-corner-action {
    opacity: 1;
}

.gds-pinned-icon {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--gds-sidebar-icon-size, 18px);
    height: var(--gds-sidebar-icon-size, 18px);
    flex-shrink: 0;
}

.gds-pinned-icon__badge {
    position: absolute;
    left: -6px;
    top: -6px;
    width: var(--gds-small-icon-size, 12px);
    height: var(--gds-small-icon-size, 12px);
    display: flex;
    align-items: center;
    justify-content: center;
}

.gds-list-surface {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
}

.gds-region-surface:focus-visible,
.gds-focus-ring:focus-visible {
    outline: 2px solid var(--gds-outline);
    outline-offset: -2px;
}

.zui-button .gds-icon {
    pointer-events: none;
    flex: none;
}

.gds-button[data-toggled="true"],
.gds-button[data-active="true"] {
    background: var(--gds-selection);
    color: var(--gds-selection-foreground);
    border-color: var(--gds-outline);
}

.gds-button[data-loading="true"] {
    opacity: 0.85;
}

.gds-icon-button {
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    position: relative;
}

.gds-icon-button:not([data-size="icon-xs"]):not([data-size="icon-sm"]):not([data-size="icon-lg"]) {
    width: var(--gds-icon-button-size, 32px);
    height: var(--gds-icon-button-size, 32px);
}

.gds-icon-button[data-toggled="true"],
.gds-icon-button[data-active="true"] {
    background: var(--gds-selection);
    color: var(--gds-selection-foreground);
}

.gds-icon-button__artwork {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    pointer-events: none;
}

.gds-icon-button__primary {
    opacity: 1;
    transform: scale(1);
    transition:
        opacity var(--gds-motion-fast, 120ms) var(--gds-motion-ease-out, ease-out),
        transform var(--gds-motion-fast, 120ms) var(--gds-motion-ease-out, ease-out);
}

.gds-icon-button__hover {
    position: absolute;
    opacity: 0;
    transform: scale(0.92);
    transition:
        opacity var(--gds-motion-fast, 120ms) var(--gds-motion-ease-out, ease-out),
        transform var(--gds-motion-fast, 120ms) var(--gds-motion-ease-out, ease-out);
}

.gds-icon-button:hover .gds-icon-button__primary,
.gds-icon-button:focus-visible .gds-icon-button__primary {
    opacity: 0;
    transform: scale(0.92);
}

.gds-icon-button:hover .gds-icon-button__hover,
.gds-icon-button:focus-visible .gds-icon-button__hover {
    opacity: 1;
    transform: scale(1);
}

.gds-app-menu-button__artwork {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: var(--gds-app-menu-icon-size, 24px);
    height: var(--gds-app-menu-icon-size, 24px);
    pointer-events: none;
}

.gds-app-menu-button__brand {
    display: flex;
    align-items: center;
    justify-content: center;
    width: var(--gds-app-menu-icon-size, 24px);
    height: var(--gds-app-menu-icon-size, 24px);
    color: var(--gds-foreground);
    opacity: 1;
    transform: scale(1);
    transition:
        opacity var(--gds-motion-fast, 120ms) var(--gds-motion-ease-out, ease-out),
        transform var(--gds-motion-fast, 120ms) var(--gds-motion-ease-out, ease-out);
}

.gds-app-menu-button__glyph {
    position: absolute;
    opacity: 0;
    transform: scale(0.85);
    color: var(--gds-foreground);
    transition:
        opacity var(--gds-motion-fast, 120ms) var(--gds-motion-ease-out, ease-out),
        transform var(--gds-motion-fast, 120ms) var(--gds-motion-ease-out, ease-out);
}

.gds-app-menu-button:hover .gds-app-menu-button__brand,
.gds-app-menu-button:focus-visible .gds-app-menu-button__brand {
    opacity: 0;
    transform: scale(0.88);
}

.gds-app-menu-button:hover .gds-app-menu-button__glyph,
.gds-app-menu-button:focus-visible .gds-app-menu-button__glyph {
    opacity: 1;
    transform: scale(1);
}

.gds-capacity {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;
}

.gds-capacity__label {
    font-size: var(--gds-font-size-xs, 12px);
    color: var(--gds-muted-foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.gds-capacity[data-selected="true"] .gds-capacity__label {
    color: var(--gds-selection-foreground);
}

.gds-capacity__track {
    width: 100%;
    height: 6px;
    border-radius: 3px;
    background: var(--gds-surface);
    border: 1px solid var(--gds-border);
    overflow: hidden;
}

.gds-capacity__fill {
    height: 100%;
    width: var(--gds-capacity-width, 0%);
    background: var(--gds-primary);
    transform: translateX(0);
    transition: width var(--gds-motion-normal, 160ms) var(--gds-motion-ease-standard, ease);
}

.gds-capacity[data-selected="true"] .gds-capacity__fill {
    background: var(--gds-selection-foreground);
}

.gds-capacity[data-unavailable="true"] .gds-capacity__fill {
    width: 0%;
    background: var(--gds-muted-foreground);
    opacity: 0.35;
}

.gds-capacity[data-loading="true"] .gds-capacity__fill {
    width: 40%;
    animation: gds-capacity-indeterminate 1200ms var(--gds-motion-ease-standard, ease) infinite;
}

@keyframes gds-capacity-indeterminate {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(250%); }
}

.gds-capacity[data-compact="true"] {
    width: auto;
    min-width: 28px;
    align-items: flex-end;
}

.gds-capacity[data-compact="true"] .gds-capacity__label {
    font-variant-numeric: tabular-nums;
}

.gds-disclosure {
    display: flex;
    flex-direction: column;
    width: 100%;
}

.gds-disclosure__trigger {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    background: transparent;
    border: none;
    padding: 4px 6px;
    border-radius: var(--gds-radius-control, 6px);
    color: var(--gds-foreground);
}

.gds-disclosure__trigger:hover {
    background: var(--gds-hover);
}

.gds-disclosure-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    color: var(--gds-muted-foreground);
    transform: rotate(0deg);
    transition: transform var(--gds-motion-normal, 160ms) var(--gds-motion-ease-out, ease-out);
}

.gds-disclosure-icon[data-state="open"] {
    transform: rotate(90deg);
    color: var(--gds-foreground);
}

.gds-link {
    color: var(--gds-link);
    cursor: pointer;
    background: transparent;
    border: none;
    padding: 0;
}

.gds-about {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 8px 4px;
    width: 100%;
    color: var(--gds-foreground);
}

.gds-about__header {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 10px;
    width: 100%;
}

.gds-about__hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    text-align: center;
    padding: 6px 0;
}

.gds-about__icon {
    width: 64px;
    height: 64px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--gds-primary);
}

.gds-about__name {
    font-size: var(--gds-font-size-xl, 20px);
    font-weight: 700;
    color: var(--gds-foreground);
}

.gds-about__tagline {
    font-size: var(--gds-font-size-sm, 14px);
    color: var(--gds-muted-foreground);
    max-width: 360px;
}

.gds-about__meta {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    justify-content: center;
    align-items: center;
    gap: 6px;
}

.gds-about__version {
    font-family: var(--gds-font-mono);
    font-size: var(--gds-font-size-xs, 12px);
    color: var(--gds-muted-foreground);
}

.gds-about__rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
}

.gds-about__row {
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 8px 10px;
    border-radius: var(--gds-radius-control, 6px);
    background: var(--gds-surface);
    border: 1px solid var(--gds-border);
}

.gds-about__section-title {
    font-size: var(--gds-font-size-md, 16px);
    font-weight: 600;
    color: var(--gds-foreground);
}

.gds-about__scroll {
    max-height: 280px;
    width: 100%;
}

.gds-about__prose {
    font-size: var(--gds-font-size-xs, 12px);
    color: var(--gds-muted-foreground);
    white-space: pre-wrap;
    line-height: 1.5;
}

.gds-about__copyright {
    font-size: var(--gds-font-size-xs, 12px);
    color: var(--gds-muted-foreground);
    text-align: center;
}

.gds-help-page {
    width: 100%;
    height: 100%;
    padding: var(--gds-content-padding, 16px);
}

.gds-help-page__body {
    display: flex;
    flex-direction: column;
    gap: 18px;
    max-width: 720px;
    width: 100%;
}

.gds-shortcut-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: var(--gds-radius-control, 6px);
    background: var(--gds-surface);
    border: 1px solid var(--gds-border);
}

.gds-shortcut-heading {
    font-size: var(--gds-font-size-md, 16px);
    font-weight: 600;
    color: var(--gds-foreground);
    padding-bottom: 4px;
}

.gds-shortcut-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 0;
}

.gds-shortcut-row__label {
    font-size: var(--gds-font-size-sm, 14px);
    color: var(--gds-foreground);
}

.gds-shortcut-row__keys {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
}

.gds-shortcut-hint {
    display: inline-flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
}

.gds-tooltip-text {
    color: var(--gds-tooltip-foreground);
    font-size: var(--gds-font-size-xs, 12px);
}

.gds-timestamp,
.gds-clock {
    font-variant-numeric: tabular-nums;
}

.gds-field-placeholder {
    color: var(--gds-muted-foreground);
    font-size: var(--gds-font-size-sm, 14px);
}

.gds-setting-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 100%;
}

.gds-setting-form[data-disabled="true"] {
    opacity: 0.6;
}

.gds-setting-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
}

.gds-setting-group {
    font-size: var(--gds-font-size-sm, 14px);
    font-weight: 600;
    color: var(--gds-muted-foreground);
    padding-top: 6px;
}

.gds-setting-row {
    display: flex;
    flex-direction: row;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 12px;
    border-radius: var(--gds-radius-control, 6px);
    background: var(--gds-surface);
    border: 1px solid var(--gds-border);
}

.gds-setting-label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    flex: 1 1 auto;
    min-width: 0;
}

.gds-setting-title {
    font-size: var(--gds-font-size-sm, 14px);
    font-weight: 500;
    color: var(--gds-foreground);
}

.gds-setting-description {
    font-size: var(--gds-font-size-xs, 12px);
    color: var(--gds-muted-foreground);
}

.gds-setting-error {
    font-size: var(--gds-font-size-xs, 12px);
    color: var(--gds-error);
}

.gds-setting-actions {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: var(--gds-setting-gap, 8px);
    flex: none;
}

.gds-setting-control {
    width: var(--gds-setting-control-width, 220px);
    display: flex;
    flex-direction: column;
    align-items: stretch;
}

.gds-setting-reset {
    width: var(--gds-setting-reset-size, 24px);
    height: var(--gds-setting-reset-size, 24px);
    display: inline-flex;
    align-items: center;
    justify-content: center;
}

.gds-setting-number {
    width: 100%;
}

.gds-setting-number-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 10px;
    width: 100%;
}

.gds-setting-number-value {
    min-width: 48px;
    text-align: right;
    font-size: var(--gds-font-size-xs, 12px);
    font-variant-numeric: tabular-nums;
    color: var(--gds-muted-foreground);
}
"#;

/// Install shared GDS structural CSS classes into the active `zgui` window document.
///
/// Safe to call repeatedly across components; installation is idempotent per document.
pub fn install_ui_stylesheets() {
    install_stylesheet(BASE_SHEET, BASE_CSS);
}
