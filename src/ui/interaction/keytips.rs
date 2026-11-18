//! Window-local mnemonic keytip registry, `Alt`-hold activation, and collision-free floating badges.

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use zgui::geom::{Css, CssPx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::reactive::{
    LocalStorage, RenderEffect, RwSignal, Signal, UnsyncCallback, on_cleanup_local,
    provide_local_context, use_local_context,
};
use zgui::view::{
    ListenerGuard, OverlayLayer, PortalProps, ShowProps, TimeoutHandle, current_host, events,
    set_timeout,
};
use zgui::vocab::{Key, KeyEvent, Modifiers, NamedKey};
use zgui::{component, css, view};

use super::shortcut::HeldModifier;

const ALT_HOLD_DELAY: Duration = Duration::from_millis(140);
const BADGE_OFFSET_Y: f32 = 4.0;
const BADGE_GAP: f32 = 4.0;
const BADGE_HEIGHT: f32 = 20.0;
const BADGE_CHAR_WIDTH: f32 = 8.0;
const BADGE_PADDING_X: f32 = 12.0;

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
        pointer-events: auto;
    }

    .gds-keytip-badge {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-width: 18px;
        height: 20px;
        padding: 0 5px;
        border-radius: 5px;
        border: 1px solid var(--gds-outline);
        background: var(--gds-surface);
        color: var(--gds-foreground);
        font-family: monospace;
        font-size: 11px;
        font-weight: 700;
        line-height: 1;
        box-shadow: 0 2px 6px rgba(0, 0, 0, 0.25);
        user-select: none;
    }

    .gds-keytip-floating[data-keytip-prefix="true"] .gds-keytip-badge {
        background: var(--gds-primary);
        color: var(--gds-selection-foreground);
        border-color: var(--gds-primary);
    }
    "#
);

/// Registered mnemonic target within the current window.
#[derive(Clone)]
pub struct KeytipBinding {
    pub chord: &'static str,
    pub target: NodeRef,
    pub on_activate: UnsyncCallback<()>,
}

/// Window-local keytip activation state and registry.
#[derive(Clone, Copy)]
pub struct KeytipRegistry {
    active: RwSignal<bool, LocalStorage>,
    entries: RwSignal<Vec<KeytipBinding>, LocalStorage>,
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

    /// Signal indicating whether mnemonic badges are currently visible.
    #[must_use]
    pub fn active(self) -> Signal<bool, LocalStorage> {
        self.active.into()
    }

    /// Signal carrying the currently typed mnemonic prefix while multi-key chords are entered.
    #[must_use]
    pub fn prefix(self) -> Signal<String, LocalStorage> {
        self.prefix.into()
    }

    /// Sets keytip visibility and clears any partial mnemonic prefix when dismissed.
    pub fn set_active(self, active: bool) {
        self.active.set(active);
        if !active {
            self.prefix.set(String::new());
        }
    }

    /// Registers a mnemonic chord target until the calling scope cleans up.
    pub fn register(self, chord: &'static str, target: NodeRef, on_activate: UnsyncCallback<()>) {
        self.entries.update(|items| {
            items.retain(|item| !item.chord.eq_ignore_ascii_case(chord));
            items.push(KeytipBinding {
                chord,
                target,
                on_activate,
            });
        });
        let entries = self.entries;
        on_cleanup_local(move || {
            entries.update(|items| items.retain(|item| !item.chord.eq_ignore_ascii_case(chord)));
        });
    }

    fn clear_prefix(self) {
        self.prefix.set(String::new());
    }
}

/// Register a mnemonic keytip in the active [`KeytipHost`], if mounted.
pub fn register_keytip(chord: &'static str, target: NodeRef, on_activate: UnsyncCallback<()>) {
    if let Some(registry) = KeytipRegistry::current() {
        registry.register(chord, target, on_activate);
    }
}

/// Outcome returned by the pure Alt-hold state tracker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AltHoldEffect {
    None,
    ArmTimer,
    CancelTimer,
    Deactivate,
}

#[derive(Default)]
struct AltHoldState {
    alt_held: bool,
    armed: bool,
}

impl AltHoldState {
    fn on_key_down(
        &mut self,
        event: &KeyEvent,
        modifiers: Modifiers,
        active: bool,
    ) -> AltHoldEffect {
        if is_pure_alt_press(event, modifiers) {
            if active {
                self.alt_held = false;
                self.armed = false;
                return AltHoldEffect::Deactivate;
            }
            if !event.repeat && !self.alt_held {
                self.alt_held = true;
                self.armed = true;
                return AltHoldEffect::ArmTimer;
            }
            return AltHoldEffect::None;
        }

        // Another key or modifier (such as AltGr / Ctrl+Alt or Alt+<key> shortcut) cancels Alt hold.
        self.alt_held = false;
        if self.armed {
            self.armed = false;
            return AltHoldEffect::CancelTimer;
        }
        AltHoldEffect::None
    }

    fn on_key_up(&mut self, key: &Key) -> AltHoldEffect {
        if is_alt_key(key) {
            self.alt_held = false;
            if self.armed {
                self.armed = false;
                return AltHoldEffect::CancelTimer;
            }
        }
        AltHoldEffect::None
    }

    fn on_timer_fired(&mut self) -> bool {
        if self.alt_held && self.armed {
            self.armed = false;
            true
        } else {
            self.armed = false;
            false
        }
    }

    fn reset(&mut self) -> AltHoldEffect {
        self.alt_held = false;
        if self.armed {
            self.armed = false;
            return AltHoldEffect::CancelTimer;
        }
        AltHoldEffect::None
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct KeytipRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl KeytipRect {
    fn intersects(self, other: Self) -> bool {
        self.x < other.x + other.width + BADGE_GAP
            && self.x + self.width + BADGE_GAP > other.x
            && self.y < other.y + other.height + BADGE_GAP
            && self.y + self.height + BADGE_GAP > other.y
    }
}

#[derive(Clone)]
struct PositionedKeytip {
    binding: KeytipBinding,
    left: f32,
    top: f32,
    label: String,
    prefix_match: bool,
}

fn normalize_chord(chord: &str) -> String {
    chord
        .split_whitespace()
        .collect::<String>()
        .to_ascii_uppercase()
}

fn estimated_badge_width(chord: &str) -> f32 {
    let chars = normalize_chord(chord).chars().count().max(1) as f32;
    (chars * BADGE_CHAR_WIDTH + BADGE_PADDING_X).max(20.0)
}

fn place_badges(
    targets: &[(Rect<CssPx, Css>, f32)],
    viewport: Option<Size<CssPx, Css>>,
) -> Vec<Point<CssPx, Css>> {
    let mut placed: Vec<KeytipRect> = Vec::with_capacity(targets.len());
    let mut out = Vec::with_capacity(targets.len());

    for &(bounds, width) in targets {
        let mut x = bounds.origin.x.0 + (bounds.size.width.0 - width).max(0.0) * 0.5;
        let mut y = bounds.origin.y.0 + bounds.size.height.0 - BADGE_HEIGHT * 0.5 + BADGE_OFFSET_Y;

        if let Some(vp) = viewport {
            x = x.clamp(4.0, (vp.width.0 - width - 4.0).max(4.0));
            y = y.clamp(4.0, (vp.height.0 - BADGE_HEIGHT - 4.0).max(4.0));
        }

        let mut candidate = KeytipRect {
            x,
            y,
            width,
            height: BADGE_HEIGHT,
        };

        let mut attempts = 0;
        while placed.iter().any(|existing| existing.intersects(candidate)) && attempts < 12 {
            attempts += 1;
            candidate.x += width + BADGE_GAP;
            if let Some(vp) = viewport
                && candidate.x + candidate.width + 4.0 > vp.width.0
            {
                candidate.x = x;
                candidate.y += BADGE_HEIGHT + BADGE_GAP;
            }
        }

        if let Some(vp) = viewport {
            candidate.x = candidate.x.clamp(4.0, (vp.width.0 - width - 4.0).max(4.0));
            candidate.y = candidate
                .y
                .clamp(4.0, (vp.height.0 - BADGE_HEIGHT - 4.0).max(4.0));
        }

        placed.push(candidate);
        out.push(Point::new(CssPx(candidate.x), CssPx(candidate.y)));
    }

    out
}

fn install_keytip_stylesheet() {
    if current_host().is_some() {
        zgui::view::install_stylesheet("gds-ui-keytips", SHEET);
    }
}

fn is_alt_key(key: &Key) -> bool {
    matches!(key, Key::Named(NamedKey::Alt))
}

fn is_pure_alt_press(event: &KeyEvent, modifiers: Modifiers) -> bool {
    if !is_alt_key(&event.key) {
        return false;
    }
    let held = modifiers.held();
    let _ = HeldModifier::from_modifiers(modifiers);
    held == Modifiers::ALT || held.is_empty()
}

enum ChordMatch {
    Activated(KeytipBinding),
    Prefix(String),
    Cancelled,
}

fn step_chord_match(bindings: &[KeytipBinding], prefix: &str, key: &Key) -> ChordMatch {
    let Some(typed) = key.as_str() else {
        return ChordMatch::Cancelled;
    };
    let next_chars: String = typed
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect();
    if next_chars.is_empty() {
        return ChordMatch::Cancelled;
    }

    let next_prefix = format!("{prefix}{next_chars}");
    if let Some(exact) = bindings
        .iter()
        .find(|binding| normalize_chord(binding.chord) == next_prefix)
        .cloned()
    {
        return ChordMatch::Activated(exact);
    }

    if bindings
        .iter()
        .any(|binding| normalize_chord(binding.chord).starts_with(&next_prefix))
    {
        return ChordMatch::Prefix(next_prefix);
    }

    ChordMatch::Cancelled
}

fn activate_binding(registry: KeytipRegistry, binding: KeytipBinding) {
    registry.set_active(false);
    binding.target.focus();
    binding.on_activate.run(());
}

fn handle_active_key_down(registry: KeytipRegistry, key: &Key, modifiers: Modifiers) -> bool {
    if *key == Key::Named(NamedKey::Escape) {
        registry.set_active(false);
        return true;
    }
    if key.is_modifier() {
        return false;
    }
    if modifiers.control() || modifiers.meta() {
        registry.set_active(false);
        return false;
    }

    let entries = registry.entries.get_untracked();
    let current_prefix = registry.prefix.get_untracked();
    match step_chord_match(&entries, &current_prefix, key) {
        ChordMatch::Activated(binding) => {
            activate_binding(registry, binding);
            true
        }
        ChordMatch::Prefix(next) => {
            registry.prefix.set(next);
            true
        }
        ChordMatch::Cancelled => {
            registry.set_active(false);
            false
        }
    }
}

fn render_positioned_keytip(registry: KeytipRegistry, item: PositionedKeytip) -> AnyView {
    let binding = item.binding.clone();
    let left = Some(format!("{}px", item.left));
    let top = Some(format!("{}px", item.top));
    let prefix_attr = item.prefix_match.then(|| "true".to_owned());
    AnyView::new(view! {
        box(
            class = "gds-keytip-floating",
            style:left = left,
            style:top = top,
            attr:data-keytip-prefix = prefix_attr,
            on:click = move |_| activate_binding(registry, binding.clone()),
        ) {
            KeytipBadge(chord = item.binding.chord)
        }
    })
}

/// Provides per-window keytip state and renders mnemonic overlay badges when active.
#[component]
pub fn KeytipHost(
    #[prop(optional)] active: Option<RwSignal<bool, LocalStorage>>,
    children: Children,
) -> impl IntoView {
    install_keytip_stylesheet();

    let active = active.unwrap_or_else(|| RwSignal::new_local(false));
    let entries = RwSignal::new_local(Vec::new());
    let prefix = RwSignal::new_local(String::new());
    let registry = KeytipRegistry {
        active,
        entries,
        prefix,
    };
    provide_local_context(registry);

    let host_ref = NodeRef::new();
    let hold_state = Rc::new(RefCell::new(AltHoldState::default()));
    let hold_timer: Rc<RefCell<Option<TimeoutHandle>>> = Rc::new(RefCell::new(None));
    let root_guards: Rc<RefCell<Vec<ListenerGuard>>> = Rc::new(RefCell::new(Vec::new()));

    if let Some(window) = zgui::prelude::try_use_window() {
        let window_focused = window.focused();
        let hold_state_focus = Rc::clone(&hold_state);
        let hold_timer_focus = Rc::clone(&hold_timer);
        let focus_watch = RenderEffect::new(move |_| {
            if !window_focused.get() {
                let effect = hold_state_focus.borrow_mut().reset();
                if effect == AltHoldEffect::CancelTimer {
                    drop(hold_timer_focus.borrow_mut().take());
                }
                registry.set_active(false);
            }
        });
        on_cleanup_local(move || drop(focus_watch));
    }

    let listener_watch = {
        let hold_state = Rc::clone(&hold_state);
        let hold_timer = Rc::clone(&hold_timer);
        let root_guards = Rc::clone(&root_guards);
        RenderEffect::new(move |_| {
            if host_ref.get().is_none() || !root_guards.borrow().is_empty() {
                return;
            }
            let Some(root) = host_ref.window_root() else {
                return;
            };

            let mut installed = Vec::new();

            {
                let hold_state = Rc::clone(&hold_state);
                let hold_timer = Rc::clone(&hold_timer);
                if let Some(guard) = root.listen(
                    events::KEY_DOWN,
                    ListenerOptions::CAPTURE,
                    move |ev: &mut EventCx<'_, events::KeyDown>| {
                        if active.get_untracked() && !is_alt_key(&ev.key) {
                            let consumed = handle_active_key_down(registry, &ev.key, ev.modifiers);
                            if consumed {
                                ev.prevent_default();
                                ev.stop_propagation();
                            }
                            return;
                        }

                        let effect = hold_state.borrow_mut().on_key_down(
                            ev,
                            ev.modifiers,
                            active.get_untracked(),
                        );

                        match effect {
                            AltHoldEffect::None => {}
                            AltHoldEffect::CancelTimer => {
                                drop(hold_timer.borrow_mut().take());
                            }
                            AltHoldEffect::Deactivate => {
                                drop(hold_timer.borrow_mut().take());
                                registry.set_active(false);
                                ev.prevent_default();
                                ev.stop_propagation();
                            }
                            AltHoldEffect::ArmTimer => {
                                drop(hold_timer.borrow_mut().take());
                                if current_host().is_some() {
                                    let hold_state_timer = Rc::clone(&hold_state);
                                    let handle = set_timeout(ALT_HOLD_DELAY, move || {
                                        if hold_state_timer.borrow_mut().on_timer_fired() {
                                            registry.clear_prefix();
                                            registry.set_active(true);
                                        }
                                    });
                                    *hold_timer.borrow_mut() = Some(handle);
                                }
                            }
                        }
                    },
                ) {
                    installed.push(guard);
                }
            }

            {
                let hold_state = Rc::clone(&hold_state);
                let hold_timer = Rc::clone(&hold_timer);
                if let Some(guard) = root.listen(
                    events::KEY_UP,
                    ListenerOptions::CAPTURE,
                    move |ev: &mut EventCx<'_, events::KeyUp>| {
                        if hold_state.borrow_mut().on_key_up(&ev.key) == AltHoldEffect::CancelTimer
                        {
                            drop(hold_timer.borrow_mut().take());
                        }
                    },
                ) {
                    installed.push(guard);
                }
            }

            {
                let hold_state = Rc::clone(&hold_state);
                let hold_timer = Rc::clone(&hold_timer);
                if let Some(guard) =
                    root.listen(events::POINTER_DOWN, ListenerOptions::CAPTURE, move |_| {
                        if hold_state.borrow_mut().reset() == AltHoldEffect::CancelTimer {
                            drop(hold_timer.borrow_mut().take());
                        }
                        if active.get_untracked() {
                            registry.set_active(false);
                        }
                    })
                {
                    installed.push(guard);
                }
            }

            *root_guards.borrow_mut() = installed;
        })
    };

    on_cleanup_local(move || {
        drop(listener_watch);
        drop(hold_timer.borrow_mut().take());
        root_guards.borrow_mut().clear();
    });

    view! {
        box(node_ref = host_ref, class = "gds-keytip-host") {
            {children.into_view_once()}
            Portal(layer = OverlayLayer::Popover) {
                Show(when = move || active.get()) {
                    box(class = "gds-keytip-overlay") {
                        for item in move || {
                            if !active.get() {
                                return Vec::new();
                            }
                            let bindings = entries.get();
                            let typed_prefix = prefix.get();
                            let mut visible_bindings = Vec::new();
                            let mut raw_rects = Vec::new();

                            let viewport = host_ref.window_root().and_then(|root| {
                                let bounds = root.window_bounds()?;
                                let scale = root.scale().max(0.01);
                                Some(Size::new(
                                    CssPx(bounds.size.width.0 / scale),
                                    CssPx(bounds.size.height.0 / scale),
                                ))
                            });

                            for binding in bindings {
                                let Some(dev_bounds) = binding.target.window_bounds() else {
                                    continue;
                                };
                                let scale = binding.target.scale().max(0.01);
                                let css_bounds = Rect::new(
                                    Point::new(
                                        CssPx(dev_bounds.origin.x.0 / scale),
                                        CssPx(dev_bounds.origin.y.0 / scale),
                                    ),
                                    Size::new(
                                        CssPx(dev_bounds.size.width.0 / scale),
                                        CssPx(dev_bounds.size.height.0 / scale),
                                    ),
                                );
                                raw_rects.push((css_bounds, estimated_badge_width(binding.chord)));
                                visible_bindings.push(binding);
                            }

                            let positions = place_badges(&raw_rects, viewport);
                            visible_bindings
                                .into_iter()
                                .zip(positions)
                                .map(|(binding, pos)| {
                                    let normalized = normalize_chord(binding.chord);
                                    let prefix_match =
                                        !typed_prefix.is_empty() && normalized.starts_with(&typed_prefix);
                                    PositionedKeytip {
                                        label: normalized,
                                        binding,
                                        left: pos.x.0,
                                        top: pos.y.0,
                                        prefix_match,
                                    }
                                })
                                .collect::<Vec<_>>()
                        }, key = |item| item.label.clone() {
                            {render_positioned_keytip(registry, item)}
                        }
                    }
                }
            }
        }
    }
}

/// Compact mnemonic badge rendered next to keytip-enabled controls.
#[component]
pub fn KeytipBadge(chord: &'static str, #[prop(into, optional)] class: Classes) -> impl IntoView {
    install_keytip_stylesheet();
    let label = normalize_chord(chord);
    view! {
        text(class = "gds-keytip-badge", class = class) {{label}}
    }
}
