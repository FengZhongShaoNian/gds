//! Floating pointer-tracked drag preview card (`Portal`) and drop target helpers.

mod content;
mod footer;
mod header;

pub use content::DragCardContent;
pub use footer::DragCardFooter;
pub use header::DragCardHeader;

use std::path::PathBuf;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, Signal, UnsyncCallback};
use zgui::view::{AttrName, OverlayLayer, Portal, events};
use zgui::vocab::PointerButton;
use zgui::{component, css, view};

/// Pointer offset in CSS pixels separating the cursor from the floating [`DragCard`].
pub const POINTER_GAP: f32 = 12.0;

/// Maximum width in CSS pixels of a floating [`DragCard`].
pub const MAX_WIDTH: f32 = 280.0;

const SHEET: &str = css!(
    r#"
    .gds-drag-portal-overlay {
        position: fixed;
        inset: 0;
        pointer-events: none;
    }

    .gds-drag-card {
        position: fixed;
        pointer-events: none;
        max-width: 280px;
        padding: 10px 12px;
        gap: 6px;
        border-radius: 8px;
        border: 1px solid var(--gds-border);
        background: var(--gds-surface);
        color: var(--gds-foreground);
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.30);
        user-select: none;
    }

    .gds-drag-card__header {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 13px;
        font-weight: 600;
        color: var(--gds-foreground);
    }

    .gds-drag-card__content {
        display: flex;
        flex-direction: column;
        gap: 2px;
        font-size: 12px;
        color: var(--gds-muted-foreground);
        overflow-wrap: anywhere;
    }

    .gds-drag-card__footer {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 11px;
        color: var(--gds-muted-foreground);
    }

    .gds-drop-target[data-drop-active="true"] {
        outline: 2px solid var(--gds-primary);
        outline-offset: -2px;
        background: var(--gds-selection);
        color: var(--gds-selection-foreground);
    }
    "#
);

pub(crate) fn install_drag_stylesheet() {
    if zgui::view::current_host().is_some() {
        crate::ui::styles::install_ui_stylesheets();
        zgui::view::install_stylesheet("gds-ui-drag", SHEET);
    }
}

/// Floating preview surface displayed via [`Portal`] while an internal pointer drag is active.
#[component]
pub fn DragCard(
    #[prop(into, default = Signal::stored_local((0.0_f32, 0.0_f32)))] position: Signal<
        (f32, f32),
        LocalStorage,
    >,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    install_drag_stylesheet();
    Portal::new(move || {
        AnyView::new(view! {
            box(class = "gds-drag-portal-overlay", a11y:hidden = true) {
                column(
                    class = "gds-drag-card gds-surface",
                    class = class,
                    style:left = move || Some(format!("{}px", position.get().0 + POINTER_GAP)),
                    style:top = move || Some(format!("{}px", position.get().1 + POINTER_GAP)),
                    {..attrs}
                ) {
                    {children.into_view_once()}
                }
            }
        })
    })
    .layer(OverlayLayer::Popover)
}

/// Window-local reactive state controller coordinating a pointer-followed drag session.
pub struct DragOverlayState<T: Clone + 'static> {
    payload: RwSignal<Option<T>, LocalStorage>,
    position: RwSignal<(f32, f32), LocalStorage>,
    hovered_target: RwSignal<Option<String>, LocalStorage>,
}

impl<T: Clone + 'static> Clone for DragOverlayState<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone + 'static> Copy for DragOverlayState<T> {}

impl<T: Clone + 'static> Default for DragOverlayState<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + 'static> DragOverlayState<T> {
    /// Creates an idle [`DragOverlayState`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            payload: RwSignal::new_local(None),
            position: RwSignal::new_local((0.0, 0.0)),
            hovered_target: RwSignal::new_local(None),
        }
    }

    /// Signal indicating whether a drag operation is currently in flight.
    #[must_use]
    pub fn is_active(self) -> Signal<bool, LocalStorage> {
        Signal::derive_local(move || self.payload.with(Option::is_some))
    }

    /// Reactive handle to the dragged payload, when active.
    #[must_use]
    pub fn payload(self) -> Signal<Option<T>, LocalStorage> {
        self.payload.into()
    }

    /// Current pointer coordinates in window CSS pixels.
    #[must_use]
    pub fn position(self) -> Signal<(f32, f32), LocalStorage> {
        self.position.into()
    }

    /// Optional identifier of the currently hovered drop target.
    #[must_use]
    pub fn hovered_target(self) -> Signal<Option<String>, LocalStorage> {
        self.hovered_target.into()
    }

    /// Starts a drag session with `payload` at CSS pointer coordinate `(x, y)`.
    pub fn begin(self, payload: T, x: f32, y: f32) {
        self.position.set((x, y));
        self.hovered_target.set(None);
        self.payload.set(Some(payload));
    }

    /// Updates the pointer coordinates during an active drag session.
    pub fn move_to(self, x: f32, y: f32) {
        self.position.set((x, y));
    }

    /// Sets the identifier of the currently hovered drop target.
    pub fn set_hovered_target(self, target: Option<String>) {
        self.hovered_target.set(target);
    }

    /// Completes the active drag session and returns its payload, clearing transient state.
    pub fn end(self) -> Option<T> {
        self.hovered_target.set(None);
        self.payload.try_update(Option::take).flatten()
    }

    /// Cancels any active drag session without delivering a drop payload.
    pub fn cancel(self) {
        self.hovered_target.set(None);
        self.payload.set(None);
    }

    /// Builds [`Attrs`] wiring pointer drag capture, movement updates, and release completion on a
    /// draggable source element.
    #[must_use]
    pub fn source_attrs(
        self,
        make_payload: impl Fn() -> Option<T> + 'static,
        on_drop: Option<UnsyncCallback<(T, (f32, f32))>>,
    ) -> Attrs {
        Attrs::new()
            .listener(
                events::POINTER_DOWN,
                ListenerOptions::DEFAULT,
                move |ev: &mut EventCx<'_, events::PointerDown>| {
                    if ev.button != Some(PointerButton::Primary) {
                        return;
                    }
                    if let Some(payload) = make_payload() {
                        ev.capture_pointer();
                        self.begin(payload, ev.position.x.0, ev.position.y.0);
                    }
                },
            )
            .listener(
                events::POINTER_MOVE,
                ListenerOptions::DEFAULT,
                move |ev: &mut EventCx<'_, events::PointerMove>| {
                    if self.payload.with_untracked(Option::is_some) {
                        self.move_to(ev.position.x.0, ev.position.y.0);
                    }
                },
            )
            .listener(
                events::POINTER_UP,
                ListenerOptions::DEFAULT,
                move |ev: &mut EventCx<'_, events::PointerUp>| {
                    if ev.button != Some(PointerButton::Primary) {
                        return;
                    }
                    let pos = (ev.position.x.0, ev.position.y.0);
                    self.move_to(pos.0, pos.1);
                    if let Some(payload) = self.end()
                        && let Some(on_drop) = on_drop
                    {
                        on_drop.run((payload, pos));
                    }
                },
            )
            .listener(
                events::POINTER_CANCEL,
                ListenerOptions::DEFAULT,
                move |_| {
                    self.cancel();
                },
            )
    }
}

/// Returns `true` when window CSS pointer coordinate `(x, y)` lies inside `node`'s rendered box.
#[must_use]
pub fn drop_hit_test(node: &NodeRef, position: (f32, f32)) -> bool {
    let Some(bounds) = node.window_bounds() else {
        return false;
    };
    let scale = node.scale().max(0.01);
    let left = bounds.origin.x.0 / scale;
    let top = bounds.origin.y.0 / scale;
    let width = bounds.size.width.0 / scale;
    let height = bounds.size.height.0 / scale;
    position.0 >= left
        && position.0 <= left + width
        && position.1 >= top
        && position.1 <= top + height
}

/// Attaches `.gds-drop-target` active state tracking and OS external file drop listeners onto a
/// destination element.
#[must_use]
pub fn drop_target_attrs(
    active: Signal<bool, LocalStorage>,
    on_external_drop: Option<UnsyncCallback<(Vec<PathBuf>, (f32, f32))>>,
) -> Attrs {
    install_drag_stylesheet();
    let external_hover = RwSignal::new_local(false);
    let is_active = Signal::derive_local(move || active.get() || external_hover.get());

    Attrs::new()
        .class_toggle(zgui::view::ClassName::new("gds-drop-target"), true)
        .attribute(AttrName::new("data-drop-active"), move || {
            is_active.get().then(|| "true".to_owned())
        })
        .listener(
            events::DRAG_ENTER,
            ListenerOptions::DEFAULT,
            move |ev: &mut EventCx<'_, events::DragEnter>| {
                if !ev.paths.is_empty() {
                    external_hover.set(true);
                    ev.prevent_default();
                }
            },
        )
        .listener(
            events::DRAG_OVER,
            ListenerOptions::DEFAULT,
            move |ev: &mut EventCx<'_, events::DragOver>| {
                if !ev.paths.is_empty() {
                    external_hover.set(true);
                    ev.prevent_default();
                }
            },
        )
        .listener(events::DRAG_LEAVE, ListenerOptions::DEFAULT, move |_| {
            external_hover.set(false);
        })
        .listener(
            events::DROP,
            ListenerOptions::DEFAULT,
            move |ev: &mut EventCx<'_, events::Drop>| {
                external_hover.set(false);
                if !ev.paths.is_empty()
                    && let Some(on_drop) = on_external_drop
                {
                    let pos = (ev.position.x.0, ev.position.y.0);
                    on_drop.run((ev.paths.clone(), pos));
                    ev.prevent_default();
                    ev.stop_propagation();
                }
            },
        )
}

#[cfg(test)]
mod tests {
    use zgui::reactive::{Mounted, install, prelude::*};

    use super::DragOverlayState;

    #[test]
    fn drag_overlay_state_tracks_begin_move_and_completion() {
        install().ok();
        let scope = Mounted::new();
        scope.with(|| {
            let drag = DragOverlayState::<&'static str>::new();
            assert!(!drag.is_active().get_untracked());

            drag.begin("3 items", 120.0, 240.0);
            assert!(drag.is_active().get_untracked());
            assert_eq!(drag.payload().get_untracked(), Some("3 items"));
            assert_eq!(drag.position().get_untracked(), (120.0, 240.0));

            drag.move_to(180.0, 260.0);
            drag.set_hovered_target(Some("Documents".to_owned()));
            assert_eq!(drag.position().get_untracked(), (180.0, 260.0));
            assert_eq!(
                drag.hovered_target().get_untracked().as_deref(),
                Some("Documents")
            );

            assert_eq!(drag.end(), Some("3 items"));
            assert!(!drag.is_active().get_untracked());
            assert_eq!(drag.hovered_target().get_untracked(), None);

            drag.begin("other", 0.0, 0.0);
            drag.cancel();
            assert!(!drag.is_active().get_untracked());
        });
        scope.unmount();
    }
}
