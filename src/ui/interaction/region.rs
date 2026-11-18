//! Keyboard focus regions and shared `1px` inset focus-contour helpers (`focus.md`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{
    LocalStorage, RenderEffect, RwSignal, Signal, on_cleanup_local, provide_local_context,
    use_local_context,
};
use zgui::view::{AttrName, ListenerGuard, current_host, events};
use zgui::vocab::{FocusCause, Role};
use zgui::{component, css, view};

use super::actions::Command;
use super::keytips;
use super::shortcut::Shortcut;

thread_local! {
    static REGION_FOCUS_VISIBLE: RefCell<HashMap<NodeId, RwSignal<bool, LocalStorage>>> =
        RefCell::new(HashMap::new());
}

const SHEET: &str = css!(
    r#"
    .gds-region-surface,
    .gds-region-surface:focus,
    .gds-region-surface:focus-visible {
        outline: 1px solid transparent;
        outline-offset: -1px;
    }

    .gds-region-surface[data-focus-visible="true"],
    .gds-region-surface[data-focus-visible="true"]:focus,
    .gds-region-surface[data-focus-visible="true"]:focus-visible {
        outline: 1px solid var(--gds-primary);
        outline-offset: -1px;
    }

    .gds-focus-ring {
        position: absolute;
        inset: 0;
        border: 1px solid var(--gds-primary);
        border-radius: 4px;
        pointer-events: none;
    }
    "#
);

#[derive(Clone, Copy)]
struct ModalityTracker {
    keyboard: RwSignal<bool, LocalStorage>,
}

fn ensure_modality_tracker(node_ref: NodeRef) -> Signal<bool, LocalStorage> {
    if let Some(existing) = zgui::reactive::is_ui_thread()
        .then(use_local_context::<ModalityTracker>)
        .flatten()
    {
        return existing.keyboard.into();
    }

    let keyboard = RwSignal::new_local(true);
    if zgui::reactive::is_ui_thread() {
        provide_local_context(ModalityTracker { keyboard });
    }

    let guards: Rc<RefCell<Vec<ListenerGuard>>> = Rc::new(RefCell::new(Vec::new()));
    let guards_watch = Rc::clone(&guards);
    let effect = RenderEffect::new(move |_| {
        if node_ref.get().is_none() || !guards_watch.borrow().is_empty() {
            return;
        }
        let Some(root) = node_ref.window_root() else {
            return;
        };
        let mut installed = Vec::new();
        if let Some(g) = root.listen(events::KEY_DOWN, ListenerOptions::CAPTURE, move |_| {
            if !keyboard.get_untracked() {
                keyboard.set(true);
            }
        }) {
            installed.push(g);
        }
        if let Some(g) = root.listen(events::POINTER_DOWN, ListenerOptions::CAPTURE, move |_| {
            if keyboard.get_untracked() {
                keyboard.set(false);
            }
        }) {
            installed.push(g);
        }
        *guards_watch.borrow_mut() = installed;
    });

    on_cleanup_local(move || {
        drop(effect);
        guards.borrow_mut().clear();
    });

    keyboard.into()
}

#[derive(Clone, Copy)]
struct RegionFocusState {
    focused: RwSignal<bool, LocalStorage>,
    focus_visible: RwSignal<bool, LocalStorage>,
    keyboard_modality: Signal<bool, LocalStorage>,
}

fn attach_region_tracking(node_ref: NodeRef) -> RegionFocusState {
    let focused = RwSignal::new_local(false);
    let focus_visible = RwSignal::new_local(false);
    let keyboard_modality = ensure_modality_tracker(node_ref);

    let registered_id: Rc<RefCell<Option<NodeId>>> = Rc::new(RefCell::new(None));
    let reg_watch = Rc::clone(&registered_id);
    let reg_fx = RenderEffect::new(move |_| {
        let next_id = node_ref.get();
        let prev_id = *reg_watch.borrow();
        if prev_id != next_id {
            REGION_FOCUS_VISIBLE.with(|map| {
                let mut map = map.borrow_mut();
                if let Some(old) = prev_id {
                    map.remove(&old);
                }
                if let Some(id) = next_id {
                    map.insert(id, focus_visible);
                }
            });
            *reg_watch.borrow_mut() = next_id;
        }
    });
    let reg_cleanup = Rc::clone(&registered_id);
    on_cleanup_local(move || {
        drop(reg_fx);
        if let Some(old) = reg_cleanup.borrow_mut().take() {
            REGION_FOCUS_VISIBLE.with(|map| {
                map.borrow_mut().remove(&old);
            });
        }
    });

    if let Some(host) = current_host() {
        let host_focus = host.focused();
        let effect = RenderEffect::new(move |_| {
            let active = host_focus.get();
            let is_keyboard = keyboard_modality.get();
            let self_id = node_ref.get();

            let inside = active.is_some_and(|id| self_id == Some(id) || node_ref.contains(id));
            if focused.get_untracked() != inside {
                focused.set(inside);
            }

            let direct = self_id.is_some() && active == self_id;
            let show = direct && is_keyboard;
            if !show && focus_visible.get_untracked() {
                focus_visible.set(false);
            }
        });
        on_cleanup_local(move || drop(effect));
    }

    RegionFocusState {
        focused,
        focus_visible,
        keyboard_modality,
    }
}

/// Returns `true` when `node_ref` itself (never a descendant!) owns keyboard focus.
#[must_use]
pub fn is_focus_visible(node_ref: NodeRef) -> bool {
    let Some(self_id) = node_ref.get() else {
        return false;
    };
    if let Some(tracked) = zgui::reactive::is_ui_thread()
        .then(|| REGION_FOCUS_VISIBLE.with(|map| map.borrow().get(&self_id).copied()))
        .flatten()
    {
        return tracked.get();
    }
    let Some(host) = current_host() else {
        return false;
    };
    let active = host.focused().get();
    let modality = zgui::reactive::is_ui_thread()
        .then(use_local_context::<ModalityTracker>)
        .flatten()
        .is_none_or(|tracker| tracker.keyboard.get());
    active == Some(self_id) && modality
}

/// Marks a region container with GDS `1px` inset keyboard focus contour (`data-focus-visible` ONLY
/// when `node_ref` itself owns keyboard focus, never when a child control bubbles `FOCUS_IN`) and
/// optional keytip staging.
#[must_use]
pub fn surface_with_shortcut(node_ref: NodeRef, shortcut: Option<Shortcut>) -> Attrs {
    if current_host().is_some() {
        crate::ui::styles::install_ui_stylesheets();
        zgui::view::install_stylesheet("gds-ui-region", SHEET);
    }

    if let Some(shortcut) = shortcut {
        keytips::anchor(node_ref, shortcut);
    }

    let state = attach_region_tracking(node_ref);
    let host_opt = current_host();

    Attrs::new()
        .class_toggle(zgui::view::ClassName::new("gds-region-surface"), true)
        .attribute(AttrName::new("data-region-focused"), move || {
            state.focused.get().then(|| "true".to_owned())
        })
        .attribute(AttrName::new("data-focus-visible"), move || {
            state.focus_visible.get().then(|| "true".to_owned())
        })
        .listener(events::FOCUS_IN, ListenerOptions::DEFAULT, move |cx| {
            state.focused.set(true);
            let direct = cx.target == cx.current;
            let show = direct
                && cx.cause != FocusCause::Pointer
                && cx.cause.shows_ring()
                && state.keyboard_modality.get_untracked();
            state.focus_visible.set(show);
        })
        .listener(events::FOCUS_OUT, ListenerOptions::DEFAULT, move |cx| {
            if cx.related.is_none() {
                state.focused.set(false);
                state.focus_visible.set(false);
            } else if let Some(ref host) = host_opt {
                let active = host.focused().get_untracked();
                let self_id = node_ref.get_untracked();
                let inside = active.is_some_and(|id| self_id == Some(id) || node_ref.contains(id));
                if !inside {
                    state.focused.set(false);
                }
                if !(self_id.is_some() && active == self_id) {
                    state.focus_visible.set(false);
                }
            } else {
                state.focus_visible.set(false);
            }
        })
}

/// Marks a region container with GDS direct-focus contour and `data-region-focused` tracking.
#[must_use]
pub fn surface(node_ref: NodeRef) -> Attrs {
    surface_with_shortcut(node_ref, None)
}

/// Inset `1px` `--gds-primary` focus overlay shared by keyboard cursor rows and notification cards.
#[component]
pub fn FocusRing(#[prop(into, optional)] class: Classes) -> impl IntoView {
    if current_host().is_some() {
        zgui::view::install_stylesheet("gds-ui-region", SHEET);
    }
    view! {
        box(class = "gds-focus-ring", class = class, a11y:hidden = true)
    }
}

/// Focusable landmark region applying `.gds-region-surface` styling, direct-focus `1px` contour,
/// and optional `Alt` shortcut keytip staging.
#[component]
pub fn Region(
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(optional)] shortcut: Option<Shortcut>,
    #[prop(optional)] command: Option<Command>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    let node_ref = node_ref.unwrap_or_default();
    let effective_shortcut = shortcut.or_else(|| command.map(Shortcut::command));
    let region_attrs = surface_with_shortcut(node_ref, effective_shortcut).merged(attrs);

    view! {
        box(
            node_ref = node_ref,
            class = class,
            tabindex = Focus::Sequential,
            a11y:role = Role::Group,
            {..region_attrs}
        ) {
            {children.into_view_once()}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use zgui::geom::{CssPx, Point};
    use zgui::platform::{AppHandler, PlatformError, Surface, SurfaceEvent};
    use zgui::prelude::*;
    use zgui::runtime::AppError;
    use zgui::view;
    use zgui::vocab::{
        Key, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction,
        PointerButton, PointerEvent, Timestamp,
    };
    use zgui_platform_headless::Harness;

    use super::{RegionProps, is_focus_visible};
    use crate::ui::Shortcut;

    #[derive(Debug, Default)]
    struct HeadlessSink {
        atlas: zgui::atlas::MemorySink,
        target: Option<zgui::render::RenderTarget>,
    }

    impl zgui::render::Renderer for HeadlessSink {
        fn capabilities(&self) -> zgui::render::RenderCapabilities {
            zgui::render::RenderCapabilities::MINIMAL
        }
        fn configure(&mut self, target: zgui::render::RenderTarget) {
            self.target = Some(target);
        }
        fn target(&self) -> Option<zgui::render::RenderTarget> {
            self.target
        }
        fn register_external(
            &mut self,
            _texture: zgui::render::ExternalTexture,
        ) -> zgui::render::TextureHandle {
            zgui::render::TextureHandle(0)
        }
        fn release_external(&mut self, _handle: zgui::render::TextureHandle) {}
        fn memory(&self) -> zgui::render::MemoryReport {
            zgui::render::MemoryReport::default()
        }
        fn draw(
            &mut self,
            _scene: &zgui::scene::Scene,
            _damage: &zgui::bits::DamageSet,
        ) -> zgui::render::FrameOutcome {
            zgui::render::FrameOutcome::Presented(zgui::render::FrameStats::default())
        }
        fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
            &mut self.atlas
        }
    }

    #[test]
    fn region_shows_contour_only_on_direct_keyboard_focus_never_on_bubbling_child_or_pointer() {
        let region_slot: Rc<RefCell<Option<NodeRef>>> = Rc::new(RefCell::new(None));
        let child_slot: Rc<RefCell<Option<NodeRef>>> = Rc::new(RefCell::new(None));
        let visible_probe: Rc<RefCell<Option<Signal<bool, zgui::reactive::LocalStorage>>>> =
            Rc::new(RefCell::new(None));

        let region_view = Rc::clone(&region_slot);
        let child_view = Rc::clone(&child_slot);
        let probe_view = Rc::clone(&visible_probe);

        let region_check = Rc::clone(&region_slot);
        let child_check = Rc::clone(&child_slot);
        let probe_check = Rc::clone(&visible_probe);

        let driver = move |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
            let mut harness = Harness::new(handler);
            harness.settle(4);

            let region_ref = region_check.borrow().expect("region ref");
            let child_ref = child_check.borrow().expect("child ref");
            let probe = probe_check.borrow().expect("visible probe");

            // 1. Tab focuses Region directly via keyboard -> focus_visible == true.
            harness.deliver_to_first(SurfaceEvent::Key {
                state: KeyState::Pressed,
                event: KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab)),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
            harness.settle(4);
            assert!(
                probe.get_untracked(),
                "direct keyboard focus on Region must enable focus_visible"
            );

            // 2. Next Tab moves focus to child control inside Region (bubbling FOCUS_IN!) ->
            // Region MUST NOT stay focus_visible!
            harness.deliver_to_first(SurfaceEvent::Key {
                state: KeyState::Pressed,
                event: KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab)),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
            harness.settle(4);
            assert!(child_ref.get_untracked().is_some(), "child control mounted");
            assert!(
                !probe.get_untracked(),
                "bubbling FOCUS_IN from child control must NEVER light ancestor Region contour"
            );

            // 3. Pointer click on Region itself -> focus_visible MUST stay false!
            let bounds = region_ref.window_bounds().expect("region bounds");
            let scale = region_ref.scale().max(1.0);
            let pt = Point::new(
                CssPx((bounds.origin.x.0 + 4.0) / scale),
                CssPx((bounds.origin.y.0 + 4.0) / scale),
            );
            for (action, ev) in [
                (PointerAction::Moved, PointerEvent::mouse(pt)),
                (
                    PointerAction::Pressed,
                    PointerEvent::mouse(pt).with_button(PointerButton::Primary),
                ),
                (
                    PointerAction::Released,
                    PointerEvent::mouse(pt).with_button(PointerButton::Primary),
                ),
            ] {
                harness.deliver_to_first(SurfaceEvent::Pointer {
                    action,
                    event: ev,
                    modifiers: Modifiers::NONE,
                    timestamp: Timestamp::ORIGIN,
                });
            }
            harness.settle(4);
            assert!(
                !probe.get_untracked(),
                "pointer focus on Region must not show keyboard contour"
            );

            let _ = Key::character("l");
            harness.shut_down();
            Ok(())
        };

        zgui::app()
            .with_title("gds-region-focus-test")
            .with_size(320.0, 200.0)
            .with_renderer(Box::new(|_surface: &Arc<dyn Surface>, target| {
                let mut renderer = HeadlessSink::default();
                zgui::render::Renderer::configure(&mut renderer, target);
                Ok::<_, AppError>(Box::new(renderer) as Box<dyn zgui::render::Renderer>)
            }))
            .run_on(driver, move || {
                let r_ref = NodeRef::new();
                let c_ref = NodeRef::new();
                *region_view.borrow_mut() = Some(r_ref);
                *child_view.borrow_mut() = Some(c_ref);
                *probe_view.borrow_mut() =
                    Some(Signal::derive_local(move || is_focus_visible(r_ref)));

                view! {
                    Region(
                        node_ref = r_ref,
                        shortcut = Shortcut::parse("Alt+L").expect("Alt+L"),
                        style:width = "200px",
                        style:height = "120px",
                    ) {
                        control(
                            node_ref = c_ref,
                            tabindex = Focus::Sequential,
                            style:display = "flex",
                            style:width = "80px",
                            style:height = "32px",
                            style:margin-top = "40px",
                        ) {
                            "Child"
                        }
                    }
                }
            })
            .expect("headless region focus test ran");
    }
}
