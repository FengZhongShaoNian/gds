use super::{Desktop, Session};

#[test]
fn plasma_and_gnome_sessions_take_precedence_over_standalone_shells() {
    assert_eq!(Desktop::from_session("KDE"), Some(Desktop::Kde));
    assert_eq!(Desktop::from_session("custom:kde"), Some(Desktop::Kde));
    assert_eq!(Desktop::from_session("GNOME"), Some(Desktop::Gnome));
    assert_eq!(
        Desktop::from_session("ubuntu:GNOME-Classic:gnome"),
        Some(Desktop::Gnome)
    );
    assert_eq!(Desktop::from_session("niri"), None);
    assert_eq!(Desktop::from_session("Hyprland"), None);
    assert_eq!(Desktop::from_session(""), None);
}

#[cfg(unix)]
#[test]
fn standalone_sessions_require_live_ipc_sockets_and_match_display_scope() {
    use std::os::unix::net::UnixListener;

    let runtime = tempfile::tempdir().expect("isolated runtime dir");
    let dir = runtime.path();

    let stale_dms = dir.join("dms-100.sock");
    let listener = UnixListener::bind(&stale_dms).expect("bind temporary DMS socket");
    drop(listener);
    assert!(stale_dms.exists());
    assert_eq!(
        Desktop::detect(&Session {
            desktop: "niri",
            runtime_dir: dir,
            wayland_display: Some("wayland-0"),
            dms_socket: None,
        }),
        None
    );

    let _live_dms = UnixListener::bind(dir.join("dms-200.sock")).expect("bind live DMS socket");
    assert_eq!(
        Desktop::detect(&Session {
            desktop: "niri",
            runtime_dir: dir,
            wayland_display: Some("wayland-0"),
            dms_socket: None,
        }),
        Some(Desktop::Dms)
    );

    let _live_noctalia =
        UnixListener::bind(dir.join("noctalia-wayland-1.sock")).expect("bind Noctalia socket");
    assert_eq!(
        Desktop::detect(&Session {
            desktop: "niri",
            runtime_dir: dir,
            wayland_display: Some("wayland-0"),
            dms_socket: None,
        }),
        Some(Desktop::Dms)
    );
    assert_eq!(
        Desktop::detect(&Session {
            desktop: "niri",
            runtime_dir: dir,
            wayland_display: Some("wayland-1"),
            dms_socket: None,
        }),
        Some(Desktop::Noctalia)
    );
    assert_eq!(
        Desktop::detect(&Session {
            desktop: "niri",
            runtime_dir: dir,
            wayland_display: None,
            dms_socket: None,
        }),
        Some(Desktop::Noctalia)
    );
    assert_eq!(
        Desktop::detect(&Session {
            desktop: "KDE",
            runtime_dir: dir,
            wayland_display: Some("wayland-1"),
            dms_socket: None,
        }),
        Some(Desktop::Kde)
    );
}
