# Runtime, Clipboard & Platform Compromises

## 1. Injecting Tokio `zbus` Connections into `ashpd`

- **Context**: GDS standardizes all application async I/O (UDisks2, Settings Portal, FileManager1, OpenURI) on a single Tokio runtime owned by `service::task_manager::Supervisor`. However, GPUI internally enables `ashpd/async-io`, and `ashpd` forbids enabling its `async-io` and `tokio` Cargo features simultaneously in the same dependency graph.
- **Compromise**: Even when compiled with `async-io`, `zbus` 5.19+ selects its Tokio backend dynamically if the `zbus::Connection` is constructed inside an active Tokio runtime context. GDS therefore constructs its shared `zbus::Connection` inside Tokio and passes it explicitly into `ashpd` requests via `OpenFileRequest::connection(...)`, avoiding `ashpd`'s process-global connection (which GPUI may have initialized on `async-io`).
- **References**: [`zbus` Tokio runtime support](https://docs.rs/zbus/5.19.0/zbus/#special-tokio-support), [GPUI async execution model](https://zed.dev/blog/zed-decoded-async-rust).

## 2. Wayland Core `wl_data_device` vs X11 File Clipboard

- **Context**: GDS exchanges copied/cut file URI lists (`text/uri-list` plus GNOME/KDE copy-cut MIME markers) and rich attachments without privileged compositor clipboard protocols.
- **Compromise**:
  - On **Wayland**, GDS uses GPUI's `read_clipboard_mime` / `write_clipboard_mime` over the standard core [`wl_data_device`](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_data_device) protocol rather than `wlr-data-control` or `ext-data-control`. Reading or claiming the clipboard requires an active keyboard focus seat and input serial (`NotFocused` is returned when unfocused; headless Weston E2E seats without physical key serials cannot claim cross-client clipboard ownership).
  - On **X11**, GPUI's `ClipboardItem` abstraction does not yet expose arbitrary multi-MIME file-transfer offers on parity with Wayland; X11 file copy/cut metadata currently operates only within the same GDS process.
- **Retirement condition**: Extend X11 `SELECTION` target negotiation in `gpui-fast-linux` when X11 desktop parity is scheduled (see [Apps & Platform Roadmap](../roadmap/apps.md)).

## 3. Niri-Only `WindowControls` Hiding vs Tiling Surface Opacity

- **Context**: GDS detects the desktop environment in two separate places with intentionally asymmetric scope:
  - **Default surface opacity** (`85%` content / `50%` sidebar vs `100%` / `85%`) recognizes tiling compositors broadly (`niri`, `sway`, `hyprland`).
  - **Hiding client-side `WindowControls`** (minimize/maximize/close) triggers **only on Niri** (`XDG_CURRENT_DESKTOP` containing `niri`, or non-empty `NIRI_SOCKET` when `XDG_CURRENT_DESKTOP` is unset) or when the compositor negotiates server-side decorations.
- **Why**: Sway and Hyprland setups vary widely in floating rules, titlebars, and keybind expectations, whereas Niri consistently manages window lifecycle without CSD buttons. Keep `WindowControls` capability-driven on non-Niri compositors rather than hiding close buttons across all tiling WMs.

## 4. Direct `shell.nix` Evaluation & `NIX_CFLAGS_LINK` Reset

- **Why `shell.nix` over bare `nix develop`**: In an uncommitted working directory or worktree with a large `target/` build directory, running bare `nix develop` against `flake.nix` can copy untracked files into `/nix/store`. `justfile` instead evaluates `import ./shell.nix` directly—reading the exact pinned revisions and NAR hashes from `flake.lock` while sharing `packaging/nix/dev-shell.nix`—zeroing store snapshot overhead.
- **Why `dev-shell.nix` resets `NIX_CFLAGS_LINK`**: Launching a shell from inside FHS wrappers (such as `steam-run`) can inject `NIX_CFLAGS_LINK="-L/usr/lib -L/usr/lib32"`, causing Nix's linker and ALSA/system libraries to mix incompatible host `glibc` symbols (e.g., `undefined reference to sqrtf@GLIBC_2.43`). Resetting `NIX_CFLAGS_LINK` inside the dev shell keeps builds hermetically bound to the locked Nix toolchain.
