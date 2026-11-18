//! Passive `Ctrl` / `Alt` hold-to-reveal shortcut hints (`shortcuts.md` §3, `motion.md`).
//!
//! Controls that own a binding stage it at their bounds while mounted. Holding `Ctrl` or `Alt`
//! immediately reveals compact suffix badges without intercepting key events (`pointer-events: none`),
//! strips the held modifier prefix (`Ctrl+R` -> `R`, `Ctrl+Shift+Tab` -> `Shift+Tab`, `Alt+A` -> `A`),
//! suppresses background workspace hints while a modal [`DialogScopeId`] is active, and clears all
//! hints immediately on modifier release or window focus loss.

#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use zgui::geom::{Css, CssPx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::reactive::{
    LocalStorage, RenderEffect, RwSignal, Signal, on_cleanup_local, provide_local_context,
    use_local_context,
};
use zgui::view::time::{FrameHandle, Timers};
use zgui::view::{ListenerGuard, OverlayLayer, PortalProps, ShowProps, current_host, events};
use zgui::vocab::{Key, Modifiers, NamedKey, Role, Timestamp};
use zgui::{component, css, view};

use super::shortcut::{HeldModifier, Shortcut};
use crate::app::Gds;

pub(crate) const LABEL_PADDING_X: f32 = 4.0;
pub(crate) const LABEL_PADDING_Y: f32 = 1.0;
pub(crate) const LABEL_TEXT_SIZE: f32 = 10.0;
pub(crate) const LABEL_LINE_HEIGHT: f32 = 1.2;
pub(crate) const EDGE_INSET: f32 = 1.0;
pub const SUFFIX_DURATION_MS: u64 = 140;
pub(crate) const SUFFIX_GAP: f32 = 6.0;
pub(crate) const LABEL_GAP: f32 = 2.0;
pub(crate) const EDGE_MARGIN: f32 = 4.0;
pub(crate) const LABEL_SEPARATION: f32 = 2.0;
pub(crate) const TRAILING_INSET: f32 = 4.0;
const MAX_ROWS: usize = 8;
const NUDGES: [f32; 9] = [0.0, 2.0, -2.0, 4.0, -4.0, 6.0, -6.0, 8.0, -8.0];
const CHAR_WIDTH_FACTOR: f32 = 0.62;

const SHEET: &str = css!(
    r#"
    .gds-keytip-host {
        display: contents;
    }

    .gds-keytip-overlay {
        position: fixed;
        inset: 0;
        pointer-events: none;
    }

    .gds-keytip-floating {
        position: fixed;
        pointer-events: none;
    }

    .gds-keytip-badge {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 1px 4px;
        border-radius: 4px;
        background: var(--gds-panel);
        border: 1px solid var(--gds-border);
        color: var(--gds-muted-foreground);
        font-family: var(--gds-font-sans);
        font-size: calc(10px * var(--gds-font-scale, 1));
        font-weight: 400;
        line-height: 1.2;
        white-space: nowrap;
        pointer-events: none;
        user-select: none;
    }

    .gds-keytip-suffix {
        display: inline-flex;
        align-items: center;
        flex-shrink: 0;
        overflow: hidden;
        pointer-events: none;
    }

    .gds-keytip-suffix__inner {
        display: inline-flex;
        align-items: center;
        flex-shrink: 0;
        pointer-events: none;
    }
    "#
);

/// Active modal dialog scope identifier inherited by controls mounted inside a modal dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DialogScopeId(pub u64);

impl DialogScopeId {
    /// Returns the enclosing modal dialog scope, if the caller is mounted inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        zgui::reactive::is_ui_thread()
            .then(use_local_context::<Self>)
            .flatten()
    }
}

thread_local! {
    static NEXT_SCOPE_ID: Cell<u64> = const { Cell::new(1) };
    static FALLBACK_MODAL_SCOPES: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn allocate_dialog_scope_id() -> u64 {
    NEXT_SCOPE_ID.with(|cell| {
        let id = cell.get();
        cell.set(id.saturating_add(1));
        id
    })
}

pub(crate) fn push_modal_scope(scope_id: u64) {
    FALLBACK_MODAL_SCOPES.with(|stack| {
        let mut stack = stack.borrow_mut();
        if !stack.contains(&scope_id) {
            stack.push(scope_id);
        }
    });
    if let Some(reg) = KeytipRegistry::current() {
        reg.modal_scopes.update(|stack| {
            if !stack.contains(&scope_id) {
                stack.push(scope_id);
            }
        });
    }
}

pub(crate) fn remove_modal_scope(scope_id: u64) {
    FALLBACK_MODAL_SCOPES.with(|stack| {
        stack.borrow_mut().retain(|&id| id != scope_id);
    });
    if let Some(reg) = KeytipRegistry::current() {
        reg.modal_scopes.update(|stack| {
            stack.retain(|&id| id != scope_id);
        });
    }
}

/// Returns `true` when at least one modal `DialogScope` is currently active in the window.
#[must_use]
pub fn has_active_modal_scope() -> bool {
    if let Some(reg) = KeytipRegistry::current() {
        return !reg.modal_scopes.get_untracked().is_empty();
    }
    FALLBACK_MODAL_SCOPES.with(|stack| !stack.borrow().is_empty())
}

pub(crate) fn scope_allows(active_modals: &[u64], hint_scope: Option<u64>) -> bool {
    match active_modals.last().copied() {
        None => hint_scope.is_none(),
        Some(top_modal) => hint_scope == Some(top_modal),
    }
}

/// Preferred anchor placement for a floating keytip badge relative to its control (`main:keytips.rs`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HintPlacement {
    /// Inside an icon button or collapsed row's top-trailing corner (`EDGE_INSET = 1px`).
    Corner,
    /// Vertically centered in a wide row's trailing space (`TRAILING_INSET = 4px`).
    Trailing,
    /// Vertically centered before a trailing action occupying `f32` px from the row's right edge.
    TrailingBefore(f32),
    /// Inside a compact row's bottom-leading corner when its top-trailing corner holds an action.
    BottomLeadingCorner,
}

/// Registered floating keytip entry within the current window.
#[derive(Clone)]
pub struct StagedHint {
    pub id: u64,
    pub target: NodeRef,
    pub shortcut: Shortcut,
    pub placement: HintPlacement,
    pub enabled: Signal<bool, LocalStorage>,
    pub dialog_scope: Option<u64>,
    pub badge_ref: NodeRef,
}

/// Window-local passive modifier-hold state and staged keytip registry.
#[derive(Clone, Copy)]
pub struct KeytipRegistry {
    held: RwSignal<Option<HeldModifier>, LocalStorage>,
    active: RwSignal<bool, LocalStorage>,
    suppressed_until_release: RwSignal<bool, LocalStorage>,
    hints: RwSignal<Vec<StagedHint>, LocalStorage>,
    modal_scopes: RwSignal<Vec<u64>, LocalStorage>,
    next_hint_id: RwSignal<u64, LocalStorage>,
    prefix: RwSignal<String, LocalStorage>,
}

impl KeytipRegistry {
    /// Returns the enclosing window's [`KeytipRegistry`], if running inside a [`KeytipHost`].
    #[must_use]
    pub fn current() -> Option<Self> {
        zgui::reactive::is_ui_thread()
            .then(use_local_context::<Self>)
            .flatten()
    }

    /// Signal carrying the currently held modifier group (`Ctrl` or `Alt`), if any.
    #[must_use]
    pub fn held(self) -> Signal<Option<HeldModifier>, LocalStorage> {
        Signal::derive_local(move || {
            if self.suppressed_until_release.get() {
                None
            } else {
                self.held.get()
            }
        })
    }

    /// Signal indicating whether any modifier group is currently active.
    #[must_use]
    pub fn active(self) -> Signal<bool, LocalStorage> {
        self.active.into()
    }

    /// Signal retained for compatibility with callers inspecting chord prefixes.
    #[must_use]
    pub fn prefix(self) -> Signal<String, LocalStorage> {
        self.prefix.into()
    }

    /// Returns `true` (reactively) when `dialog_scope` is allowed to display hints right now.
    #[must_use]
    pub fn is_scope_active(self, dialog_scope: Option<u64>) -> bool {
        let modals = self.modal_scopes.get();
        scope_allows(&modals, dialog_scope)
    }

    /// Sets keytip visibility or clears the held modifier state.
    pub fn set_active(self, next_active: bool) {
        if next_active {
            self.suppressed_until_release.set(false);
            if self.held.get_untracked().is_none() {
                self.held.set(Some(HeldModifier::Alt));
            }
            self.active.set(true);
        } else {
            self.held.set(None);
            self.active.set(false);
            self.prefix.set(String::new());
        }
    }

    /// Updates the held modifier group immediately on key press/release.
    pub fn set_held(self, next: Option<HeldModifier>) {
        self.suppressed_until_release.set(false);
        if self.held.get_untracked() != next {
            self.held.set(next);
        }
        let is_active = next.is_some();
        if self.active.get_untracked() != is_active {
            self.active.set(is_active);
        }
    }

    /// Clears any active modifier-hold state immediately (e.g. on window focus loss).
    pub fn release(self) {
        self.set_held(None);
    }

    /// Stages a [`Shortcut`] on `target` using `placement` until the calling reactive scope cleans up.
    pub fn stage(
        self,
        target: NodeRef,
        shortcut: Shortcut,
        placement: HintPlacement,
        enabled: Signal<bool, LocalStorage>,
    ) {
        let id = self.next_hint_id.get_untracked();
        self.next_hint_id.set(id.saturating_add(1));
        let dialog_scope = DialogScopeId::current().map(|s| s.0);
        let badge_ref = NodeRef::new();

        self.hints.update(|list| {
            list.retain(|entry| entry.target != target || entry.shortcut != shortcut);
            list.push(StagedHint {
                id,
                target,
                shortcut,
                placement,
                enabled,
                dialog_scope,
                badge_ref,
            });
        });

        let hints = self.hints;
        on_cleanup_local(move || {
            hints.update(|list| list.retain(|entry| entry.id != id));
        });
    }
}

/// Stages a [`Shortcut`] on `target` with explicit [`HintPlacement`] and reactive `enabled` guard.
pub fn stage_shortcut(
    target: NodeRef,
    shortcut: impl Into<Shortcut>,
    placement: HintPlacement,
    enabled: Signal<bool, LocalStorage>,
) {
    if let Some(registry) = KeytipRegistry::current() {
        registry.stage(target, shortcut.into(), placement, enabled);
    }
}

/// Stages a top-trailing corner keytip badge (`HintPlacement::Corner`) on `target`.
pub fn anchor(target: NodeRef, shortcut: impl Into<Shortcut>) {
    stage_shortcut(
        target,
        shortcut,
        HintPlacement::Corner,
        Signal::stored_local(true),
    );
}

/// Stages a top-trailing corner keytip badge gated by `enabled`.
pub fn anchor_when(
    target: NodeRef,
    shortcut: impl Into<Shortcut>,
    enabled: Signal<bool, LocalStorage>,
) {
    stage_shortcut(target, shortcut, HintPlacement::Corner, enabled);
}

/// Stages a row-trailing keytip badge (`HintPlacement::Trailing`) on `target`.
pub fn row_anchor(target: NodeRef, shortcut: impl Into<Shortcut>) {
    stage_shortcut(
        target,
        shortcut,
        HintPlacement::Trailing,
        Signal::stored_local(true),
    );
}

/// Stages a row-trailing keytip badge before a trailing control of width `reserved` px.
pub fn trailing_before_anchor(target: NodeRef, shortcut: impl Into<Shortcut>, reserved: f32) {
    stage_shortcut(
        target,
        shortcut,
        HintPlacement::TrailingBefore(reserved),
        Signal::stored_local(true),
    );
}

/// Stages a compact row's bottom-leading corner keytip badge (`HintPlacement::BottomLeadingCorner`).
pub fn bottom_corner_anchor(target: NodeRef, shortcut: impl Into<Shortcut>) {
    stage_shortcut(
        target,
        shortcut,
        HintPlacement::BottomLeadingCorner,
        Signal::stored_local(true),
    );
}

/// Computes the modifier bit contributed by pressing/releasing `key` itself so Wayland, winit,
/// and headless backends agree regardless of whether `ModifiersChanged` arrives before or after `Key`.
#[must_use]
pub(crate) fn modifier_bit_for_key(key: &Key) -> Modifiers {
    match key {
        Key::Named(NamedKey::Control) => Modifiers::CONTROL,
        Key::Named(NamedKey::Alt) => Modifiers::ALT,
        Key::Named(NamedKey::AltGraph) => Modifiers::CONTROL | Modifiers::ALT,
        Key::Named(NamedKey::Shift) => Modifiers::SHIFT,
        Key::Named(NamedKey::Meta | NamedKey::Super) => Modifiers::META,
        _ => Modifiers::NONE,
    }
}

#[must_use]
pub(crate) fn held_after_key_down(key: &Key, modifiers: Modifiers) -> Option<HeldModifier> {
    if matches!(key, Key::Named(NamedKey::AltGraph)) {
        return None;
    }
    let effective = modifiers.held() | modifier_bit_for_key(key);
    HeldModifier::from_modifiers(effective)
}

#[must_use]
pub(crate) fn held_after_key_up(key: &Key, modifiers: Modifiers) -> Option<HeldModifier> {
    let effective = modifiers.held() & !modifier_bit_for_key(key);
    HeldModifier::from_modifiers(effective)
}

/// Measured or estimated slot awaiting collision-free placement inside `content`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LabelSlot {
    pub anchor: Rect<CssPx, Css>,
    pub size: Size<CssPx, Css>,
    pub placement: HintPlacement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Below,
    Above,
}

fn rect_left(r: Rect<CssPx, Css>) -> f32 {
    r.origin.x.0
}

fn rect_top(r: Rect<CssPx, Css>) -> f32 {
    r.origin.y.0
}

fn rect_right(r: Rect<CssPx, Css>) -> f32 {
    r.origin.x.0 + r.size.width.0
}

fn rect_bottom(r: Rect<CssPx, Css>) -> f32 {
    r.origin.y.0 + r.size.height.0
}

fn rect_center_x(r: Rect<CssPx, Css>) -> f32 {
    r.origin.x.0 + r.size.width.0 * 0.5
}

fn rect_center_y(r: Rect<CssPx, Css>) -> f32 {
    r.origin.y.0 + r.size.height.0 * 0.5
}

pub(crate) fn rects_intersect(a: Rect<CssPx, Css>, b: Rect<CssPx, Css>) -> bool {
    rect_left(a) < rect_right(b)
        && rect_right(a) > rect_left(b)
        && rect_top(a) < rect_bottom(b)
        && rect_bottom(a) > rect_top(b)
}

fn free(origin: Point<CssPx, Css>, size: Size<CssPx, Css>, placed: &[Rect<CssPx, Css>]) -> bool {
    let dilated = Rect::new(
        Point::new(
            CssPx(origin.x.0 - LABEL_SEPARATION),
            CssPx(origin.y.0 - LABEL_SEPARATION),
        ),
        Size::new(
            CssPx(size.width.0 + LABEL_SEPARATION * 2.0),
            CssPx(size.height.0 + LABEL_SEPARATION * 2.0),
        ),
    );
    placed
        .iter()
        .all(|existing| !rects_intersect(dilated, *existing))
}

fn clamp_axis(origin: f32, length: f32, start: f32, end: f32) -> f32 {
    origin.clamp(
        start + EDGE_MARGIN,
        (end - EDGE_MARGIN - length).max(start + EDGE_MARGIN),
    )
}

fn fits(left: f32, width: f32, content: Rect<CssPx, Css>) -> bool {
    left >= rect_left(content) + EDGE_MARGIN && left + width <= rect_right(content) - EDGE_MARGIN
}

fn shifted_columns(preferred: f32, slot: LabelSlot, content: Rect<CssPx, Css>) -> Vec<f32> {
    let mut columns: Vec<f32> = NUDGES
        .into_iter()
        .map(|nudge| preferred + nudge)
        .filter(|left| fits(*left, slot.size.width.0, content))
        .collect();
    if columns.is_empty() {
        columns.push(clamp_axis(
            preferred,
            slot.size.width.0,
            rect_left(content),
            rect_right(content),
        ));
    }
    columns
}

fn corner_origins(slot: LabelSlot, content: Rect<CssPx, Css>) -> Vec<Point<CssPx, Css>> {
    let top = clamp_axis(
        rect_top(slot.anchor) + EDGE_INSET,
        slot.size.height.0,
        rect_top(content),
        rect_bottom(content),
    );
    let preferred = rect_right(slot.anchor) - EDGE_INSET - slot.size.width.0;
    shifted_columns(preferred, slot, content)
        .into_iter()
        .map(|left| Point::new(CssPx(left), CssPx(top)))
        .collect()
}

fn trailing_origin(slot: LabelSlot, content: Rect<CssPx, Css>) -> Option<Point<CssPx, Css>> {
    if slot.anchor.size.width.0 < slot.size.width.0 + TRAILING_INSET * 2.0
        || slot.anchor.size.height.0 < slot.size.height.0
    {
        return None;
    }
    Some(Point::new(
        CssPx(clamp_axis(
            rect_right(slot.anchor) - TRAILING_INSET - slot.size.width.0,
            slot.size.width.0,
            rect_left(content),
            rect_right(content),
        )),
        CssPx(clamp_axis(
            rect_center_y(slot.anchor) - slot.size.height.0 * 0.5,
            slot.size.height.0,
            rect_top(content),
            rect_bottom(content),
        )),
    ))
}

fn trailing_before_origins(
    slot: LabelSlot,
    content: Rect<CssPx, Css>,
    reserved: f32,
) -> Vec<Point<CssPx, Css>> {
    if slot.anchor.size.height.0 < slot.size.height.0 {
        return Vec::new();
    }
    let step = slot.size.width.0 + LABEL_SEPARATION;
    let top = clamp_axis(
        rect_center_y(slot.anchor) - slot.size.height.0 * 0.5,
        slot.size.height.0,
        rect_top(content),
        rect_bottom(content),
    );
    let mut origins = Vec::new();
    let mut left = rect_right(slot.anchor) - reserved - slot.size.width.0;
    while left >= rect_left(slot.anchor) && fits(left, slot.size.width.0, content) {
        origins.push(Point::new(CssPx(left), CssPx(top)));
        left -= step;
    }
    origins
}

fn bottom_leading_origins(slot: LabelSlot, content: Rect<CssPx, Css>) -> Vec<Point<CssPx, Css>> {
    let top = clamp_axis(
        rect_bottom(slot.anchor) - EDGE_INSET - slot.size.height.0,
        slot.size.height.0,
        rect_top(content),
        rect_bottom(content),
    );
    let preferred = rect_left(slot.anchor) + EDGE_INSET;
    let mut columns: Vec<f32> = NUDGES
        .into_iter()
        .filter(|nudge| *nudge >= 0.0)
        .map(|nudge| preferred + nudge)
        .filter(|left| fits(*left, slot.size.width.0, content))
        .collect();
    if columns.is_empty() {
        columns.push(clamp_axis(
            preferred,
            slot.size.width.0,
            rect_left(content),
            rect_right(content),
        ));
    }
    columns
        .into_iter()
        .map(|left| Point::new(CssPx(left), CssPx(top)))
        .collect()
}

fn edge_top(slot: LabelSlot, content: Rect<CssPx, Css>, side: Side, row: usize) -> Option<f32> {
    let height = slot.size.height.0;
    let step = height + LABEL_SEPARATION;
    let top = match side {
        Side::Below if row == 0 => rect_bottom(slot.anchor) - height * 0.5,
        Side::Below => {
            rect_bottom(slot.anchor) + height * 0.5 + LABEL_GAP + step * (row - 1) as f32
        }
        Side::Above if row == 0 => rect_top(slot.anchor) - height * 0.5,
        Side::Above => {
            rect_top(slot.anchor) - height * 0.5 - LABEL_GAP - height - step * (row - 1) as f32
        }
    };
    (top >= rect_top(content) + EDGE_MARGIN && top + height <= rect_bottom(content) - EDGE_MARGIN)
        .then_some(top)
}

fn edge_columns(slot: LabelSlot, content: Rect<CssPx, Css>) -> Vec<f32> {
    shifted_columns(
        rect_right(slot.anchor) - EDGE_INSET - slot.size.width.0,
        slot,
        content,
    )
}

fn candidates(slot: LabelSlot, content: Rect<CssPx, Css>) -> Vec<Point<CssPx, Css>> {
    let mut list = match slot.placement {
        HintPlacement::Corner => corner_origins(slot, content),
        HintPlacement::Trailing => trailing_origin(slot, content).into_iter().collect(),
        HintPlacement::TrailingBefore(reserved) => trailing_before_origins(slot, content, reserved),
        HintPlacement::BottomLeadingCorner => bottom_leading_origins(slot, content),
    };
    for side in [Side::Below, Side::Above] {
        for row in 0..MAX_ROWS {
            let Some(top) = edge_top(slot, content, side, row) else {
                break;
            };
            list.extend(
                edge_columns(slot, content)
                    .into_iter()
                    .map(|left| Point::new(CssPx(left), CssPx(top))),
            );
        }
    }
    list
}

fn overflow_origin(
    slot: LabelSlot,
    content: Rect<CssPx, Css>,
    placed: &[Rect<CssPx, Css>],
) -> Point<CssPx, Css> {
    let left = rect_left(content) + EDGE_MARGIN;
    let right = rect_right(content) - EDGE_MARGIN;
    let top = rect_top(content) + EDGE_MARGIN;
    let bottom = rect_bottom(content) - EDGE_MARGIN;
    let step_x = (slot.size.width.0 + LABEL_SEPARATION).max(1.0);
    let step_y = (slot.size.height.0 + LABEL_SEPARATION).max(1.0);
    let mut y = top;
    while y + slot.size.height.0 <= bottom {
        let mut x = left;
        while x + slot.size.width.0 <= right {
            let origin = Point::new(CssPx(x), CssPx(y));
            if free(origin, slot.size, placed) {
                return origin;
            }
            x += step_x;
        }
        y += step_y;
    }
    Point::new(
        CssPx(clamp_axis(
            rect_center_x(slot.anchor) - slot.size.width.0 * 0.5,
            slot.size.width.0,
            rect_left(content),
            rect_right(content),
        )),
        CssPx(clamp_axis(
            rect_bottom(slot.anchor) + LABEL_GAP,
            slot.size.height.0,
            rect_top(content),
            rect_bottom(content),
        )),
    )
}

/// Places ordered `slots` inside `content` without overlap (`main:keytips.rs:1235`).
#[must_use]
pub(crate) fn place_labels(
    slots: &[LabelSlot],
    content: Rect<CssPx, Css>,
) -> Vec<Point<CssPx, Css>> {
    let mut placed: Vec<Rect<CssPx, Css>> = Vec::with_capacity(slots.len());
    let mut origins = Vec::with_capacity(slots.len());
    for &slot in slots {
        let origin = candidates(slot, content)
            .into_iter()
            .find(|origin| free(*origin, slot.size, &placed))
            .unwrap_or_else(|| overflow_origin(slot, content, &placed));
        placed.push(Rect::new(origin, slot.size));
        origins.push(origin);
    }
    origins
}

fn typography_scale_untracked() -> f32 {
    Gds::try_current().map_or(1.0, |gds| {
        gds.appearance().typography().scaled_untracked(1.0)
    })
}

fn estimated_badge_size(label: &str) -> Size<CssPx, Css> {
    let scale = typography_scale_untracked();
    let text_px = LABEL_TEXT_SIZE * scale;
    let chars = label.chars().count().max(1) as f32;
    let width = (chars * text_px * CHAR_WIDTH_FACTOR + LABEL_PADDING_X * 2.0 + 2.0).ceil();
    let height = (text_px * LABEL_LINE_HEIGHT + LABEL_PADDING_Y * 2.0 + 2.0).ceil();
    Size::new(CssPx(width.max(14.0)), CssPx(height.max(14.0)))
}

fn motion_reduced() -> bool {
    Gds::try_current().is_some_and(|gds| gds.appearance().motion().reduced())
}

fn motion_reduced_untracked() -> bool {
    Gds::try_current().is_some_and(|gds| {
        let _z = zgui::reactive::enter_non_reactive_zone();
        gds.appearance().motion().reduced()
    })
}

fn ease_out_cubic(t: f32) -> f32 {
    let clamped = t.clamp(0.0, 1.0);
    1.0 - (1.0 - clamped).powi(3)
}

struct SuffixAnim {
    from: f32,
    to: f32,
    started_at: Option<Timestamp>,
    last_frame_at: Option<Timestamp>,
    frame_handle: Option<FrameHandle>,
}

/// Frame-driven `140ms` `ease-out cubic` scalar interpolator with smooth mid-flight retargeting,
/// immediate snap under reduced motion, and zero frame requests once settled (`motion.md`).
pub fn use_inline_transition(
    target: Signal<f32, LocalStorage>,
    duration_ms: u64,
) -> Signal<f32, LocalStorage> {
    let initial = target.get_untracked();
    let current = RwSignal::new_local(initial);
    let timers = Timers::current();
    let state = Rc::new(RefCell::new(SuffixAnim {
        from: initial,
        to: initial,
        started_at: None,
        last_frame_at: None,
        frame_handle: None,
    }));

    fn schedule_next(
        timers: &Timers,
        state: &Rc<RefCell<SuffixAnim>>,
        current: RwSignal<f32, LocalStorage>,
        duration_ms: u64,
    ) {
        let timers_clone = timers.clone();
        let state_clone = Rc::clone(state);
        let handle = timers.request_frame(move |at| {
            if motion_reduced_untracked() || duration_ms == 0 {
                let final_to = {
                    let mut anim = state_clone.borrow_mut();
                    anim.started_at = None;
                    anim.last_frame_at = None;
                    anim.frame_handle = None;
                    anim.from = anim.to;
                    anim.to
                };
                current.set(final_to);
                return;
            }

            let (next_val, settled) = {
                let mut anim = state_clone.borrow_mut();
                let start = match anim.started_at {
                    Some(start) => start,
                    None => {
                        let base = anim.last_frame_at.unwrap_or(at);
                        anim.started_at = Some(base);
                        base
                    }
                };
                anim.last_frame_at = Some(at);
                let elapsed_secs = (at - start).as_secs_f32();
                let total_secs = (duration_ms as f32) / 1000.0;
                let progress = (elapsed_secs / total_secs).clamp(0.0, 1.0);
                if progress >= 1.0 {
                    let final_to = anim.to;
                    anim.from = final_to;
                    anim.started_at = None;
                    anim.last_frame_at = None;
                    anim.frame_handle = None;
                    (final_to, true)
                } else {
                    let eased = ease_out_cubic(progress);
                    let interpolated = anim.from + (anim.to - anim.from) * eased;
                    (interpolated, false)
                }
            };

            current.set(next_val);
            if !settled {
                schedule_next(&timers_clone, &state_clone, current, duration_ms);
            }
        });
        state.borrow_mut().frame_handle = Some(handle);
    }

    let watch_state = Rc::clone(&state);
    let effect = RenderEffect::new(move |_| {
        let goal = target.get();
        let reduced = motion_reduced();
        let cur = current.get_untracked();

        if reduced || duration_ms == 0 || timers.is_none() {
            let canceled = {
                let mut anim = watch_state.borrow_mut();
                anim.from = goal;
                anim.to = goal;
                anim.started_at = None;
                anim.last_frame_at = None;
                anim.frame_handle.take()
            };
            drop(canceled);
            if (cur - goal).abs() > f32::EPSILON {
                current.set(goal);
            }
            return;
        }

        let needs_schedule = {
            let mut anim = watch_state.borrow_mut();
            if (anim.to - goal).abs() <= 1e-4 {
                false
            } else {
                anim.from = cur;
                anim.to = goal;
                anim.started_at = None;
                if (cur - goal).abs() <= 1e-4 {
                    anim.last_frame_at = None;
                    let old = anim.frame_handle.take();
                    drop(old);
                    false
                } else {
                    anim.frame_handle.is_none()
                }
            }
        };

        if needs_schedule && let Some(ref timers) = timers {
            schedule_next(timers, &watch_state, current, duration_ms);
        }
    });

    let cleanup_state = Rc::clone(&state);
    on_cleanup_local(move || {
        drop(effect);
        let handle = cleanup_state.borrow_mut().frame_handle.take();
        drop(handle);
    });

    current.into()
}

pub(crate) fn install_keytip_stylesheet() {
    if current_host().is_some() {
        zgui::view::install_stylesheet("gds-ui-keytips", SHEET);
    }
}

#[derive(Clone)]
struct PositionedKeytip {
    id: u64,
    badge_ref: NodeRef,
    left: f32,
    top: f32,
    label: String,
}

fn render_positioned_keytip(item: PositionedKeytip) -> AnyView {
    let left = format!("{:.1}px", item.left.round());
    let top = format!("{:.1}px", item.top.round());
    let aria = item.label.clone();
    AnyView::new(view! {
        box(
            node_ref = item.badge_ref,
            class = "gds-keytip-floating",
            style:left = left,
            style:top = top,
            a11y:role = Role::Label,
            a11y:label = aria,
        ) {
            text(class = "gds-keytip-badge") {{item.label}}
        }
    })
}

/// In-flow `140ms` width-animated keytip suffix rendered beside text [`crate::ui::button::Button`] labels.
#[component]
pub fn KeytipSuffix(
    shortcut: Shortcut,
    #[prop(into, default = Signal::stored_local(true))] enabled: Signal<bool, LocalStorage>,
) -> impl IntoView {
    install_keytip_stylesheet();
    let registry = KeytipRegistry::current();
    let my_scope = DialogScopeId::current().map(|s| s.0);
    let last_label: RwSignal<Option<String>, LocalStorage> = RwSignal::new_local(None);

    let active_label = Signal::derive_local({
        let shortcut = shortcut.clone();
        move || {
            let reg = registry?;
            if !enabled.get() || !reg.is_scope_active(my_scope) {
                return None;
            }
            let held = reg.held().get()?;
            shortcut.compact_label(held)
        }
    });

    let cache_fx = RenderEffect::new(move |_| {
        if let Some(label) = active_label.get()
            && last_label.get_untracked().as_deref() != Some(label.as_str())
        {
            last_label.set(Some(label));
        }
    });
    on_cleanup_local(move || drop(cache_fx));

    let target = Signal::derive_local(move || {
        if active_label.get().is_some() {
            1.0
        } else {
            0.0
        }
    });
    let progress = use_inline_transition(target, SUFFIX_DURATION_MS);

    let inner_ref = NodeRef::new();
    let show = Signal::derive_local(move || {
        (progress.get() > 0.001 || active_label.get().is_some()) && last_label.get().is_some()
    });
    let observed_inner = inner_ref.observe_border_size_while(move || show.get());

    view! {
        Show(when = move || show.get()) {
            box(
                class = "gds-keytip-suffix",
                a11y:hidden = true,
                style:width = move || {
                    let scale = Gds::try_current()
                        .map_or(1.0, |gds| gds.appearance().typography().scaled(1.0));
                    let gap_px = (SUFFIX_GAP * scale).round();
                    let label_text = last_label.get().unwrap_or_default();
                    let fallback_width = estimated_badge_size(&label_text).width.0 + gap_px;
                    let total_width = observed_inner
                        .get()
                        .map(|sz| (sz.width.0 / inner_ref.scale().max(0.01)).ceil())
                        .filter(|&w| w > 1.0)
                        .unwrap_or(fallback_width);
                    let p = progress.get();
                    Some(format!("{:.2}px", (total_width * p).max(0.0)))
                },
            ) {
                box(
                    node_ref = inner_ref,
                    class = "gds-keytip-suffix__inner",
                    style:padding-left = move || {
                        let scale = Gds::try_current()
                            .map_or(1.0, |gds| gds.appearance().typography().scaled(1.0));
                        Some(format!("{:.1}px", (SUFFIX_GAP * scale).round()))
                    },
                    style:opacity = move || Some(format!("{:.3}", progress.get().clamp(0.0, 1.0))),
                    a11y:role = Role::Label,
                    a11y:label = move || last_label.get().unwrap_or_default(),
                ) {
                    text(class = "gds-keytip-badge") {{move || last_label.get().unwrap_or_default()}}
                }
            }
        }
    }
}

/// Provides per-window passive `Ctrl` / `Alt` hold-to-reveal state and renders collision-free
/// floating keytip badges (`OverlayLayer::Toast`, `pointer-events: none`).
#[component]
pub fn KeytipHost(
    #[prop(optional)] active: Option<RwSignal<bool, LocalStorage>>,
    children: Children,
) -> impl IntoView {
    install_keytip_stylesheet();

    let held = RwSignal::new_local(None);
    let active = active.unwrap_or_else(|| RwSignal::new_local(false));
    let suppressed_until_release = RwSignal::new_local(false);
    let hints = RwSignal::new_local(Vec::new());
    let initial_modals = FALLBACK_MODAL_SCOPES.with(|stack| stack.borrow().clone());
    let modal_scopes = RwSignal::new_local(initial_modals);
    let next_hint_id = RwSignal::new_local(1);
    let prefix = RwSignal::new_local(String::new());

    let registry = KeytipRegistry {
        held,
        active,
        suppressed_until_release,
        hints,
        modal_scopes,
        next_hint_id,
        prefix,
    };
    provide_local_context(registry);

    // If an external caller clears `active` to `false` (e.g. opening a menu overlay), suppress
    // the current hold until the next key transition without looping.
    let sync_active_fx = RenderEffect::new(move |prev_active: Option<bool>| {
        let now_active = active.get();
        if prev_active == Some(true)
            && !now_active
            && held.get_untracked().is_some()
            && modal_scopes.get_untracked().is_empty()
        {
            suppressed_until_release.set(true);
        }
        now_active
    });
    on_cleanup_local(move || drop(sync_active_fx));

    if let Some(window) = zgui::prelude::try_use_window() {
        let focus_edge_guard = window.on_focus_change(move |focused| {
            if !focused {
                registry.release();
            }
        });
        let window_focused = window.focused();
        let focus_watch = RenderEffect::new(move |_| {
            if !window_focused.get() {
                registry.release();
            }
        });
        on_cleanup_local(move || {
            drop(focus_edge_guard);
            drop(focus_watch);
        });
    }

    let host_ref = NodeRef::new();
    let root_guards: Rc<RefCell<Vec<ListenerGuard>>> = Rc::new(RefCell::new(Vec::new()));

    let listener_watch = {
        let root_guards = Rc::clone(&root_guards);
        RenderEffect::new(move |_| {
            if host_ref.get().is_none() || !root_guards.borrow().is_empty() {
                return;
            }
            let Some(root) = host_ref.window_root() else {
                return;
            };

            let mut installed = Vec::new();
            if let Some(guard) = root.listen(
                events::KEY_DOWN,
                ListenerOptions::CAPTURE,
                move |ev: &mut EventCx<'_, events::KeyDown>| {
                    let next = held_after_key_down(&ev.key, ev.modifiers);
                    registry.set_held(next);
                },
            ) {
                installed.push(guard);
            }

            if let Some(guard) = root.listen(
                events::KEY_UP,
                ListenerOptions::CAPTURE,
                move |ev: &mut EventCx<'_, events::KeyUp>| {
                    let next = held_after_key_up(&ev.key, ev.modifiers);
                    registry.set_held(next);
                },
            ) {
                installed.push(guard);
            }

            *root_guards.borrow_mut() = installed;
        })
    };

    on_cleanup_local(move || {
        drop(listener_watch);
        root_guards.borrow_mut().clear();
    });

    let effective_held = registry.held();

    view! {
        box(node_ref = host_ref, class = "gds-keytip-host") {
            {children.into_view_once()}
            Portal(layer = OverlayLayer::Toast) {
                Show(when = move || effective_held.get().is_some()) {
                    box(class = "gds-keytip-overlay") {
                        for item in move || {
                            let Some(held_mod) = effective_held.get() else {
                                return Vec::new();
                            };
                            let active_modals = modal_scopes.get();
                            let staged = hints.get();

                            let window_root = host_ref.window_root();
                            let toast_occlusion =
                                crate::ui::feedback::notifications::active_stack_occlusion(
                                    window_root.and_then(|r| r.get_untracked()),
                                );
                            let content = window_root
                                .and_then(|root| {
                                    let bounds = root.window_bounds()?;
                                    let scale = root.scale().max(0.01);
                                    Some(Rect::new(
                                        Point::new(
                                            CssPx(bounds.origin.x.0 / scale),
                                            CssPx(bounds.origin.y.0 / scale),
                                        ),
                                        Size::new(
                                            CssPx(bounds.size.width.0 / scale),
                                            CssPx(bounds.size.height.0 / scale),
                                        ),
                                    ))
                                })
                                .unwrap_or_else(|| {
                                    Rect::new(
                                        Point::new(CssPx(0.0), CssPx(0.0)),
                                        Size::new(CssPx(1280.0), CssPx(800.0)),
                                    )
                                });

                            let mut candidates_list = Vec::new();
                            for entry in staged {
                                if !scope_allows(&active_modals, entry.dialog_scope) {
                                    continue;
                                }
                                if !entry.enabled.get() {
                                    continue;
                                }
                                let Some(label) = entry.shortcut.compact_label(held_mod) else {
                                    continue;
                                };
                                let Some(dev_bounds) = entry.target.window_bounds() else {
                                    continue;
                                };
                                let scale = entry.target.scale().max(0.01);
                                let anchor = Rect::new(
                                    Point::new(
                                        CssPx(dev_bounds.origin.x.0 / scale),
                                        CssPx(dev_bounds.origin.y.0 / scale),
                                    ),
                                    Size::new(
                                        CssPx(dev_bounds.size.width.0 / scale),
                                        CssPx(dev_bounds.size.height.0 / scale),
                                    ),
                                );
                                if anchor.size.width.0 <= 0.0 || anchor.size.height.0 <= 0.0 {
                                    continue;
                                }

                                let badge_size = entry
                                    .badge_ref
                                    .window_bounds()
                                    .map(|b| {
                                        let s = entry.badge_ref.scale().max(0.01);
                                        Size::new(
                                            CssPx((b.size.width.0 / s).ceil()),
                                            CssPx((b.size.height.0 / s).ceil()),
                                        )
                                    })
                                    .filter(|sz| sz.width.0 > 1.0 && sz.height.0 > 1.0)
                                    .unwrap_or_else(|| estimated_badge_size(&label));

                                if let Some((toast_ref, toast_rect)) = toast_occlusion {
                                    let is_toast_hint = entry.target == toast_ref
                                        || entry
                                            .target
                                            .get_untracked()
                                            .is_some_and(|id| toast_ref.contains(id));
                                    if !is_toast_hint {
                                        let control_occluded =
                                            anchor.size.width.0 <= toast_rect.size.width.0
                                                && anchor.size.height.0 <= toast_rect.size.height.0
                                                && rects_intersect(anchor, toast_rect);
                                        let slot = LabelSlot {
                                            anchor,
                                            size: badge_size,
                                            placement: entry.placement,
                                        };
                                        let badge_occluded = candidates(slot, content)
                                            .first()
                                            .is_some_and(|origin| {
                                                rects_intersect(
                                                    Rect::new(*origin, badge_size),
                                                    toast_rect,
                                                )
                                            });
                                        if control_occluded || badge_occluded {
                                            continue;
                                        }
                                    }
                                }

                                candidates_list.push((entry, label, anchor, badge_size));
                            }

                            // Deterministic visual order: top-to-bottom, then left-to-right, then id.
                            candidates_list.sort_by(|left, right| {
                                left.2
                                    .origin
                                    .y
                                    .0
                                    .total_cmp(&right.2.origin.y.0)
                                    .then_with(|| left.2.origin.x.0.total_cmp(&right.2.origin.x.0))
                                    .then_with(|| left.0.id.cmp(&right.0.id))
                            });

                            let slots: Vec<LabelSlot> = candidates_list
                                .iter()
                                .map(|(entry, _, anchor, size)| LabelSlot {
                                    anchor: *anchor,
                                    size: *size,
                                    placement: entry.placement,
                                })
                                .collect();

                            let origins = place_labels(&slots, content);
                            candidates_list
                                .into_iter()
                                .zip(origins)
                                .map(|((entry, label, _, _), pos)| PositionedKeytip {
                                    id: entry.id,
                                    badge_ref: entry.badge_ref,
                                    left: pos.x.0,
                                    top: pos.y.0,
                                    label,
                                })
                                .collect::<Vec<_>>()
                        }, key = |item| item.id {
                            {render_positioned_keytip(item)}
                        }
                    }
                }
            }
        }
    }
}

/// Compact mnemonic badge rendered next to keytip-enabled controls.
#[component]
pub fn KeytipBadge(
    #[prop(into)] chord: String,
    #[prop(into, optional)] class: Classes,
) -> impl IntoView {
    install_keytip_stylesheet();
    view! {
        text(class = "gds-keytip-badge", class = class) {{chord}}
    }
}
