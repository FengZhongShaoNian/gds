use zgui::reactive::{Mounted, RwSignal, UnsyncCallback, install, prelude::*};

use super::{KeytipHost, KeytipRegistry};

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
