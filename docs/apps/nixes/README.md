# GDS Nixes

GDS Nixes is a native GPUI browser for flake-based NixOS and Home Manager configurations. It shares GDS's window frame, sidebar, settings rows, live-reloaded semantic theme engine (`theme::Observer`), typography, and Fluent translations.

## Running

```sh
just dev nixes
just dev nixes --flake /path/to/configuration
```

Nix must be available on `PATH`. Choose **Open flake** and select a local directory containing `flake.nix` (an existing `flake.lock` is recommended; evaluation may fetch declared inputs, but GDS Nixes never creates or modifies lockfiles, builds activation derivations, or switches generations).

## Discovery & Read-Only Preview

- **Configuration discovery**: Discovers both standalone `homeConfigurations.<user>` outputs and NixOS-integrated `nixosConfigurations.<host>.config.home-manager.users.<user>` outputs. For NixOS hosts, an evaluation-only module inspects each user via `extendModules` so user imports, `extraSpecialArgs`, shared modules, and host-dependent definitions resolve accurately without writing files into the flake.
- **Enabled-branch pruning**: The sidebar lists discovered users and enabled module groups (`enable == true`, including modules enabled by default). Disabled module subtrees and internal options are pruned inside Nix before exporting metadata; `home` retains core identity fields (`username`, `homeDirectory`, `stateVersion`, `packages`, `sessionVariables`).
- **Value inspection**: Search filters option paths and descriptions across the active user's enabled modules. Booleans render as read-only switches, scalars as value fields, lists as item/package summaries, and attribute sets as key summaries, with full descriptions, option paths, types, and defaults in hover tooltips. Unset options are distinguished from evaluation errors.
- **Isolated background evaluation**: Runs on Tokio outside the UI thread with a 3-minute timeout. Catchable per-option evaluation failures are isolated via `tryEval`; top-level evaluation errors present expandable diagnostic logs with one-click retry.

## Implementation & Validation

Code in [`bin/nixes/src/`](../../../bin/nixes/src/) follows FSD (`app`, `pages/home`, `entities/home`, `shared/i18n`). Embedded Nix expressions pass user paths and attribute names strictly as escaped Nix strings rather than interpolated shell commands.

Run integration tests (requires `nix` on `PATH`):

```sh
cargo test --bin gds-nixes --features nixes
```

Planned type-aware editing and override-module milestones are tracked in [Application Roadmap](../../roadmap/apps.md).
