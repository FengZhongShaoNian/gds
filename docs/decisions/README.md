# Technical Decisions & Active Compromises

This directory records **active architectural trade-offs, upstream patches, and platform workarounds** in GDS—explaining *why* non-obvious constraints exist today and *under what conditions* a compromise can be retired.

Current system behavior belongs in the main documentation (`docs/development.md`, `docs/visual-language/`, `docs/apps/`), and unimplemented plans belong in [docs/roadmap/](../roadmap/README.md).

## Index

| Document | Scope |
| --- | --- |
| [Upstream Patches & Dependency Pins](upstream-patches.md) | Why `gpui-component`, `gpui-fast`, and `gpui-fast-linux` run from `GanDesktop/*` Git branches, plus `smb = 0.10.2` and `rust-embed` constraints |
| [Runtime, Clipboard & Platform Compromises](runtime-and-platform.md) | Injecting Tokio `zbus` into `ashpd`, Wayland `wl_data_device` focus serials vs X11 clipboard scope, Niri window controls vs tiling opacity, and `shell.nix` / `NIX_CFLAGS_LINK` build guards |
| [Terminal PTY Loop & Directory Policy](terminal-pty-loop.md) | Why `service::terminal` implements its own Tokio `AsyncFd` + `SIGCHLD` session loop instead of Alacritty's upstream event loop, and why directory switching never injects `cd` into live shells |
| [External Tool Integrations](external-integrations.md) | CLI-based Baloo/KIO tag queries vs linking Qt/C++, external archive delegation (`OpenURI` vs CLI `7zz`), and non-NixOS `--nix` OpenGL limitations |
