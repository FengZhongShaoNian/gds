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

.gds-icon-button {
    width: var(--gds-icon-button-size, 32px);
    height: var(--gds-icon-button-size, 32px);
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
}

.gds-icon-button[data-toggled="true"] {
    background: var(--gds-selection);
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
    background: var(--gds-primary);
}
"#;

/// Install shared GDS structural CSS classes into the active `zgui` window document.
///
/// Safe to call repeatedly across components; installation is idempotent per document.
pub fn install_ui_stylesheets() {
    install_stylesheet(BASE_SHEET, BASE_CSS);
}
