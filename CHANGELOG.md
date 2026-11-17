# Changelog

All notable changes to officially released GDS packages (**GDS Files** and **GDS Portal**) are documented in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.0.5] - 2026-11-17

### Changed

- **GDS Files cold-start performance**: Significantly reduced startup overhead with targeted Wayland Vulkan initialization and cached Fontconfig font matching.

### Fixed

- Fixed fatal panics in GDS Files caused by re-entrant UI entity borrows, including table selection and navigation callbacks.

## [0.0.4] - 2026-11-15

### Fixed

- **Noctalia v5 Out-of-the-Box Theme Synchronization**:
  - Automatically stage `$XDG_CACHE_HOME/gds/theme/noctalia-template.json`, provision the managed `${NOCTALIA_CONFIG_HOME:-$XDG_CONFIG_HOME}/noctalia/gds.toml` drop-in (preserving user edits and symlinks), and trigger `"config-reload"` / `"templates-apply"` over Noctalia's live IPC socket so System theme following is available and synchronized on first launch without manual registration.
  - Excluded `noctalia-dmenu-*.sock` helper sockets during fallback Noctalia IPC socket discovery when `WAYLAND_DISPLAY` is unset.

## [0.0.3] - 2026-11-15

### Added

- **File Tags & Audio Previews**:
  - File tagging via `user.xdg.tags` extended attributes and hierarchical `tags:/` catalog browsing backed by KDE Baloo/KIO (`Settings → Extensions → Tags`).
  - Audio tag metadata and embedded cover art previews in the details panel alongside image and video thumbnails.
- **Browsing & Interaction**:
  - Type-to-search directory filtering with keyboard result navigation.
  - Compact multi-item drag preview cards and middle-click tab closing.
  - Dedicated **Performance** settings page (behind a confirmation gate) for progressive directory loading, metadata probing, preview caching, and transfer tuning.
- **Themes**:
  - Noctalia v5 system theme, light/dark mode, and 22-color terminal palette synchronization via the bundled `share/gds-files/noctalia/gds.json` template, alongside live IPC socket detection for Noctalia and DankMaterialShell in standalone Wayland sessions.
  - Custom user theme `[metadata]` support with localized `i18n` display names in `$XDG_CONFIG_HOME/gds/themes/`.
- **Installation & Packaging**:
  - Interactive multi-distro installer (`https://gds.gandesktop.dev/install.sh`) backed by Cloudflare R2 (`pkgs.gandesktop.dev`) with `MANIFEST`/`SHA256SUMS` verification, `--with-portal`, selective `--uninstall` (`--portal-only`, `--purge`), and legacy package migration.
  - Home Manager modules (`homeManagerModules.{default,gds-files,gds-portal}`) and automated release publishing across Arch, Debian 13, Ubuntu 24.04/26.04, Fedora 43/44, and NUR/AUR.

### Changed

- **Cold-Start & Directory Performance**:
  - Replaced the external Python/GTK GNOME theme helper with a pure in-process Rust reader (XDG Settings Portal, Adwaita baselines, and GTK 3/4 CSS) and deferred non-critical startup services.
  - Added streaming directory snapshots, progressive metadata enrichment, `O(1)` range selection, and cached sort projections for large folders.
- **UX & Packaging**:
  - Shared SQLite location and view-state storage across multiple Files windows, auto-closed exited embedded terminal tabs, skipped Recent-history writes when restoring pinned tabs on launch, removed the inline play button overlay from video grid tiles, and consolidated hidden-file toggling to view-level `Ctrl+H`.
  - Coordinated background task supervision and graceful service draining before application exit.
  - Trimmed native package runtime dependencies and scoped `services.gds-portal` strictly to `FileChooser` routing.

### Removed

- Legacy `qt5ct` and `qt6ct` theme sources, superseded by unified desktop system providers (`Noctalia`, `DankMaterialShell`, `KDE Plasma`, and `GNOME`) and shared TOML user themes.

### Fixed

- Preserved configured window and sidebar surface opacity when external system themes are unavailable or fail validation.
- Restored external modal parent-window associations across picker and dialog teardown, and aligned multiline tooltip hint spacing.
- Validated undo target identities before mutation, hardened MP4 header seeking, preserved modification times (`mtime`) during file transfers, and rebound directory watchers when watched paths are replaced.

## [0.0.2] - 2026-11-07

### Added

- Recursive Unix permission editing and context-sensitive action menus in GDS Files.
- Custom user TOML theme loading from `$XDG_CONFIG_HOME/gds/themes/` with live hot-reload.
- Isolated headless Wayland end-to-end workflow test runner (`just test-files`).

### Fixed

- Scaled thumbnails using decoder target dimensions and added retry handling for resource-limited preview tasks.

## [0.0.1] - 2026-11-04

### Added

- Initial preview release of **GDS Files** and **GDS Portal** (`xdg-desktop-portal-gds`) for Linux + Wayland:
  - Multi-tab local file browser with virtualized table/grid views, breadcrumbs, undo history, FreeDesktop Trash 1.0, UDisks2 volume management, and native SFTP/SMB client support.
  - Native CLI file picker and `org.freedesktop.impl.portal.FileChooser` D-Bus portal backend with Wayland/X11 parent-window modality.
  - Embedded local PTY terminal dock with live theme palette synchronization.
  - Shared visual language supporting System (DMS, KDE Plasma, GNOME) and 6 built-in light/dark palettes, configurable surface opacity, rounded corners, interface motion controls, and English / Simplified Chinese localization.
  - Native packaging for Nix / NixOS, Fedora RPM, Debian / Ubuntu DEB, and Arch Linux.

[0.0.5]: https://github.com/GanDesktop/gds/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/GanDesktop/gds/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/GanDesktop/gds/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/GanDesktop/gds/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/GanDesktop/gds/releases/tag/v0.0.1
