# Upstream Patches & Dependency Pins

GDS never vendors third-party crate sources into this repository. When an upstream crate lacks required desktop protocol hooks or introduces ecosystem version conflicts, we maintain minimal patch branches under `GanDesktop/*` (or pin compatible versions in `Cargo.toml`), lock exact commits in `Cargo.lock` and `packaging/nix/cargo-lock.nix`, and retire each override once upstream releases an equivalent fix.

## Active Git Patch Branches

 GDS builds GPUI Kit `0.9.0` with its `gpui-fast` backend (`0.1.13` baseline):

| Dependency | Patched Branch / Commit | Why Patched | Retirement Condition |
| --- | --- | --- | --- |
| `gpui-component` | [`GanDesktop/gpui-kit` @ `gds-0.9.0`](https://github.com/GanDesktop/gpui-kit/tree/gds-0.9.0) | Adds configurable Linux client-side window corner radius (`10 px`), synchronizing content clipping and platform input hit regions while keeping `0 px` square corners as default. Other `gpui-kit` crates stay on crates.io `0.9.0`. | Upstream `gpui-kit` exposes configurable Linux CSD corner clipping and hit-region synchronization. |
| `gpui-fast` | [`GanDesktop/gpui-fast` @ `gds-0.1.13`](https://github.com/GanDesktop/gpui-fast/tree/gds-0.1.13) | Exposes public window options for external parent window handles (Portal FileChooser modality) and caller startup activation tokens (`StartupId`), and restores mouse input modality upon external file drag-enter after keyboard navigation. | Upstream `gpui-fast` publishes external parent/activation APIs and drag-enter modality recovery. |
| `gpui-fast-linux` | [`GanDesktop/gpui-fast-linux` @ `2270030`](https://github.com/GanDesktop/gpui-fast-linux/tree/2270030d3e0cf75def11e44e5cc0f6c9991fae49) | Implements Wayland (`xdg-foreign-v2`, `xdg-dialog`) and X11 (`WM_TRANSIENT_FOR`) external parent bindings, retaining the parent association until modal window destruction completes so KWin clears parent dimming reliably. | Upstream `gpui-fast-linux` lands matching Wayland/X11 transient teardown semantics. |

> [!NOTE]
> Because `gpui-kit` enables features additively, `gpui-pre` may still appear in `cargo tree` metadata alongside `gpui-fast` (`gpui-pre-http-client` also shares HTTP client types). Only `gpui-fast` is linked and executed at runtime. Whenever updating `gds-0.1.13` or `gds-0.9.0`, update both `Cargo.lock` and `packaging/nix/cargo-lock.nix`.

## Pinned Crate Versions & Features

- **`smb = "=0.10.2"` (with `russh = 0.65.0`, `russh-sftp = 3.0.1`)**: Newer `smb` releases pull in `RustCrypto` prerelease crates that conflict with `russh`'s stable `RustCrypto` dependency graph. Keep `smb` pinned until `russh` and `smb` align on the same `RustCrypto` major generation.
- **`rust-embed` (`debug-embed`)**: Dev and release packages embed application icons and Fluent `.ftl` catalogs directly into the compiled binary so packaged RPM/DEB/Arch `dev`-profile builds never require the build host's `target/` or source directory at runtime.
