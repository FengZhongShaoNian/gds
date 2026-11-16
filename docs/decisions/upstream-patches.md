# Upstream Forks & Dependency Pins

GDS never vendors third-party crate sources into this repository. Instead of maintaining detached crates.io tarball overrides under `[patch.crates-io]`, GDS directly tracks versioned forks under `GanDesktop/*` in `Cargo.toml`, locks exact commits in `Cargo.lock` and `packaging/nix/cargo-lock.nix`, and retires each fork branch once upstream releases an equivalent fix.

## Active Git Forks

GDS builds GPUI Kit `0.10.0` (`gds-0.10.0`) directly against its `gpui-fast` (`0.1.16` / `gds-0.1.16`) monorepo fork:

| Dependency | Fork Repository / Branch | Why Forked | Retirement Condition |
| --- | --- | --- | --- |
| `gpui-kit` | [`GanDesktop/gpui-kit` @ `gds-0.10.0`](https://github.com/GanDesktop/gpui-kit/tree/gds-0.10.0) (`f61ebe1b`) | Forked from `longbridge/gpui-kit` `v0.10.0` (`f697c313`). Upstream `v0.10.0` adopted configurable Linux client-side corner radii (`Theme::window_radius`), so `gds-0.10.0` only points workspace GPUI dependencies directly to `GanDesktop/gpui-fast`. | Upstream `gpui-kit` depends on an upstream `gpui-fast` release containing all required GDS platform and Vulkan fixes. |
| `gpui-fast` (`gpui`, `gpui_linux`, `gpui_wgpu`, `gpui_platform`, `compat/gpui-pre*`) | [`GanDesktop/gpui-fast` @ `gds-0.1.16`](https://github.com/GanDesktop/gpui-fast/tree/gds-0.1.16) (`8e6e750c`) | Forked from `longbridge/gpui-fast` `v0.1.16` (`454c29e`). Exposes external Wayland (`xdg-foreign-v2`, `xdg-dialog`) and X11 (`WM_TRANSIENT_FOR`) parent window handles across modal teardown, startup activation tokens (`StartupId`), drag-enter mouse modality recovery, core Wayland clipboard MIME transport, single-roundtrip `zwp_linux_dmabuf_v1` `dev_t` probing, and hybrid-GPU `VK_LUNARG_direct_driver_loading` initialization (see [Hybrid-GPU Direct Vulkan Initialization](hybrid-gpu-vulkan-init.md)). | Upstream `longbridge/gpui-fast` and `wgpu` ([gfx-rs/wgpu#9912](https://github.com/gfx-rs/wgpu/issues/9912)) merge matching platform integration and targeted Vulkan initialization semantics. |

> [!NOTE]
> Because `gpui-kit` enables features additively across its compatibility packages (`compat/gpui-pre*`), `gpui-pre` aliases resolve directly to `GanDesktop/gpui-fast` alongside `gpui-fast` in `Cargo.lock` with zero duplicate crates.io GPUI snapshots. Whenever updating `gds-0.1.16` or `gds-0.10.0`, update both `Cargo.lock` and `packaging/nix/cargo-lock.nix`.

## Pinned Crate Versions & Features

- **`smb = "=0.10.2"` (with `russh = 0.65.0`, `russh-sftp = 3.0.1`)**: Newer `smb` releases pull in `RustCrypto` prerelease crates that conflict with `russh`'s stable `RustCrypto` dependency graph. Keep `smb` pinned until `russh` and `smb` align on the same `RustCrypto` major generation.
- **`rust-embed` (`debug-embed`)**: Dev and release packages embed application icons and Fluent `.ftl` catalogs directly into the compiled binary so packaged RPM/DEB/Arch `dev`-profile builds never require the build host's `target/` or source directory at runtime.
