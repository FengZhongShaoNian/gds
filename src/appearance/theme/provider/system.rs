//! Follow the active desktop's palette and appearance preferences.
mod dms;
mod gnome;
mod kde;
mod material;
mod noctalia;

use directories::BaseDirs;
use gpui_kit::WindowAppearance;
use std::path::{Path, PathBuf};

use super::{Error, Loaded, ModePreference, read};

const DEFAULT_WAYLAND_DISPLAY: &str = "wayland-0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Desktop {
    Dms,
    Noctalia,
    Kde,
    Gnome,
}

struct Session<'a> {
    desktop: &'a str,
    runtime_dir: &'a Path,
    wayland_display: Option<&'a str>,
    dms_socket: Option<&'a Path>,
}

impl Desktop {
    fn current(dirs: &BaseDirs) -> Option<Self> {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let runtime = runtime_dir(dirs);
        let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
        let dms_socket = std::env::var_os("DMS_SOCKET").map(PathBuf::from);
        Self::detect(&Session {
            desktop: &desktop,
            runtime_dir: &runtime,
            wayland_display: wayland_display.as_deref(),
            dms_socket: dms_socket.as_deref(),
        })
    }

    fn detect(session: &Session<'_>) -> Option<Self> {
        Self::from_session(session.desktop).or_else(|| Self::active_shell(session))
    }

    fn from_session(desktop: &str) -> Option<Self> {
        for name in desktop.split(':') {
            if name.eq_ignore_ascii_case("KDE") {
                return Some(Self::Kde);
            }
            if name.eq_ignore_ascii_case("GNOME") {
                return Some(Self::Gnome);
            }
        }
        None
    }

    #[cfg(unix)]
    fn active_shell(session: &Session<'_>) -> Option<Self> {
        if noctalia_alive(session.runtime_dir, session.wayland_display) {
            return Some(Self::Noctalia);
        }
        if dms_alive(session.runtime_dir, session.dms_socket) {
            return Some(Self::Dms);
        }
        None
    }

    #[cfg(not(unix))]
    fn active_shell(_session: &Session<'_>) -> Option<Self> {
        None
    }

    fn path(self, dirs: &BaseDirs) -> PathBuf {
        match self {
            Self::Kde => dirs.config_dir().join("kdeglobals"),
            Self::Dms => dirs.cache_dir().join("DankMaterialShell/dms-colors.json"),
            Self::Noctalia => dirs.cache_dir().join("gds/theme/noctalia.json"),
            Self::Gnome => dirs.config_dir().join("dconf/user"),
        }
    }
}

fn runtime_dir(dirs: &BaseDirs) -> PathBuf {
    dirs.runtime_dir()
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir)
}

fn noctalia_socket(runtime_dir: &Path, display: &str) -> PathBuf {
    runtime_dir.join(format!("noctalia-{display}.sock"))
}

#[cfg(unix)]
fn socket_alive(path: &Path) -> bool {
    std::os::unix::net::UnixStream::connect(path).is_ok()
}

#[cfg(unix)]
fn noctalia_alive(runtime_dir: &Path, wayland_display: Option<&str>) -> bool {
    if let Some(display) = wayland_display.filter(|value| !value.is_empty()) {
        return socket_alive(&noctalia_socket(runtime_dir, display));
    }
    if socket_alive(&noctalia_socket(runtime_dir, DEFAULT_WAYLAND_DISPLAY)) {
        return true;
    }
    has_live_socket(runtime_dir, |name| {
        name.strip_prefix("noctalia-")
            .and_then(|rest| rest.strip_suffix(".sock"))
            .is_some_and(|display| !display.is_empty())
    })
}

#[cfg(unix)]
fn dms_alive(runtime_dir: &Path, dms_socket: Option<&Path>) -> bool {
    if let Some(socket) = dms_socket.filter(|path| !path.as_os_str().is_empty()) {
        return socket_alive(socket);
    }
    has_live_socket(runtime_dir, |name| {
        let Some(stem) = name.strip_suffix(".sock") else {
            return false;
        };
        matches!(stem, "dms" | "danklinux")
            || stem.starts_with("dms-")
            || stem.starts_with("danklinux-")
    })
}

#[cfg(unix)]
fn has_live_socket(runtime_dir: &Path, matches_name: impl Fn(&str) -> bool) -> bool {
    let Ok(entries) = std::fs::read_dir(runtime_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        if file_name.to_str().is_some_and(&matches_name) && socket_alive(&entry.path()) {
            return true;
        }
    }
    false
}

pub(crate) fn is_gnome() -> bool {
    Desktop::from_session(&std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default())
        == Some(Desktop::Gnome)
}

pub(super) fn uses_platform_mode() -> bool {
    is_gnome()
}

pub(super) fn source_paths() -> Vec<PathBuf> {
    let Some(dirs) = BaseDirs::new() else {
        return Vec::new();
    };
    match Desktop::from_session(&std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()) {
        Some(Desktop::Kde) => vec![Desktop::Kde.path(&dirs)],
        Some(Desktop::Gnome) => vec![
            Desktop::Gnome.path(&dirs),
            dirs.config_dir().join("gtk-4.0/gtk.css"),
            dirs.config_dir().join("gtk-4.0/settings.ini"),
            dirs.config_dir().join("gtk-3.0/gtk.css"),
            dirs.config_dir().join("gtk-3.0/settings.ini"),
        ],
        Some(Desktop::Dms | Desktop::Noctalia) => {
            unreachable!("standalone shells are detected via runtime IPC")
        }
        None => {
            let runtime = runtime_dir(&dirs);
            let display = std::env::var("WAYLAND_DISPLAY")
                .ok()
                .filter(|value| !value.is_empty());
            let noctalia_sock = noctalia_socket(
                &runtime,
                display.as_deref().unwrap_or(DEFAULT_WAYLAND_DISPLAY),
            );
            vec![
                Desktop::Noctalia.path(&dirs),
                Desktop::Dms.path(&dirs),
                noctalia_sock,
            ]
        }
    }
}

/// # Errors
/// Reports missing user directories, inactive desktop shells, unreadable files,
/// or invalid desktop palettes.
pub(super) fn load(
    preference: ModePreference,
    platform: WindowAppearance,
) -> Result<Loaded, Error> {
    let dirs = BaseDirs::new().ok_or(Error::NoUserDirectories)?;
    let desktop = Desktop::current(&dirs).ok_or(Error::UnsupportedDesktop)?;
    if desktop == Desktop::Gnome {
        return gnome::load(preference, platform, &dirs);
    }
    let path = desktop.path(&dirs);
    let contents = read(&path)?;
    match desktop {
        Desktop::Kde => kde::parse(&contents, &path, preference),
        Desktop::Dms => {
            let parsed = dms::parse(&contents, &path, preference)?;
            Ok(Loaded {
                palette: parsed.palette,
                mode: parsed.mode,
                source_path: Some(path),
            })
        }
        Desktop::Noctalia => {
            let parsed = noctalia::parse(&contents, &path, preference)?;
            Ok(Loaded {
                palette: parsed.palette,
                mode: parsed.mode,
                source_path: Some(path),
            })
        }
        Desktop::Gnome => unreachable!(
            "GNOME uses Portal and GTK configuration rather than a single palette file"
        ),
    }
}

#[cfg(test)]
mod tests;
