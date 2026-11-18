use zgui::geom::{CssPx, Point, Rect, Size};
use zgui::reactive::{Mounted, RwSignal, UnsyncCallback, install, prelude::*};
use zgui::vocab::{Key, KeyCode, KeyEvent, Modifiers, NamedKey, PhysicalKey};

use super::{
    AltHoldEffect, AltHoldState, KeytipBinding, KeytipHost, KeytipRegistry, place_badges,
    step_chord_match,
};
use crate::ui::shortcut::HeldModifier;

#[test]
fn keytip_registry_tracks_active_state_and_entries() {
    install().ok();
    let scope = Mounted::new();
    scope.with(|| {
        let active = RwSignal::new_local(false);
        let _host = KeytipHost(Some(active), zgui::view::Children::from_view(()));
        let reg = KeytipRegistry::current().expect("keytip registry installed");
        assert!(!reg.active().get_untracked());
        reg.set_active(true);
        assert!(reg.active().get_untracked());
        reg.register(
            "F",
            zgui::prelude::NodeRef::new(),
            UnsyncCallback::new(|()| {}),
        );
    });
    scope.unmount();
}

#[test]
fn alt_hold_ignores_altgr_and_shortcut_chords() {
    let mut state = AltHoldState::default();
    let alt_down = KeyEvent::named(NamedKey::Alt, PhysicalKey::Code(KeyCode::AltLeft));

    // Ctrl+Alt (AltGr on international keyboards) must not arm keytips.
    assert_eq!(
        HeldModifier::from_modifiers(Modifiers::CONTROL | Modifiers::ALT),
        HeldModifier::None
    );
    assert_eq!(
        state.on_key_down(&alt_down, Modifiers::CONTROL | Modifiers::ALT, false),
        AltHoldEffect::None
    );
    assert!(!state.on_timer_fired());

    // Pure Alt press arms the 140ms hold timer.
    assert_eq!(
        state.on_key_down(&alt_down, Modifiers::ALT, false),
        AltHoldEffect::ArmTimer
    );

    // Pressing another key before timeout cancels keytip activation.
    let key_n = KeyEvent::character("n");
    assert_eq!(
        state.on_key_down(&key_n, Modifiers::ALT, false),
        AltHoldEffect::CancelTimer
    );
    assert!(!state.on_timer_fired());

    // Holding Alt until the timer fires activates keytips.
    assert_eq!(
        state.on_key_down(&alt_down, Modifiers::ALT, false),
        AltHoldEffect::ArmTimer
    );
    assert!(state.on_timer_fired());

    // Pressing Alt again while active toggles keytips off.
    assert_eq!(
        state.on_key_down(&alt_down, Modifiers::ALT, true),
        AltHoldEffect::Deactivate
    );
}

#[test]
fn place_badges_nudges_overlapping_targets_without_overlap() {
    let target_a = Rect::new(
        Point::new(CssPx(40.0), CssPx(40.0)),
        Size::new(CssPx(28.0), CssPx(24.0)),
    );
    let target_b = target_a;
    let viewport = Size::new(CssPx(320.0), CssPx(240.0));

    let positions = place_badges(&[(target_a, 24.0), (target_b, 24.0)], Some(viewport));
    assert_eq!(positions.len(), 2);
    assert_ne!(positions[0], positions[1]);
    assert!(positions[1].x.0 >= positions[0].x.0 + 24.0 || positions[1].y.0 > positions[0].y.0);
}

#[test]
fn step_chord_match_handles_prefix_and_exact_activation() {
    install().ok();
    let scope = Mounted::new();
    scope.with(|| {
        let bindings = vec![
            KeytipBinding {
                chord: "F",
                target: zgui::prelude::NodeRef::new(),
                on_activate: UnsyncCallback::new(|()| {}),
            },
            KeytipBinding {
                chord: "G D",
                target: zgui::prelude::NodeRef::new(),
                on_activate: UnsyncCallback::new(|()| {}),
            },
        ];

        assert!(matches!(
            step_chord_match(&bindings, "", &Key::character("g")),
            super::ChordMatch::Prefix(prefix) if prefix == "G"
        ));
        assert!(matches!(
            step_chord_match(&bindings, "G", &Key::character("d")),
            super::ChordMatch::Activated(binding) if binding.chord == "G D"
        ));
        assert!(matches!(
            step_chord_match(&bindings, "", &Key::character("z")),
            super::ChordMatch::Cancelled
        ));
    });
    scope.unmount();
}
