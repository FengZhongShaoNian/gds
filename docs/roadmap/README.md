# GDS Roadmap

Planned capabilities and deferred upstream optimizations across the GDS suite. Items documented here are **not yet implemented**; shipped behavior lives in the main application and architecture docs.

## Roadmap Areas

| Area | Summary | Details |
| --- | --- | --- |
| **[GDS Files](files.md)** | Built-in archive browsing & extraction, SFTP/SMB move & share discovery, recursive search, batch rename, and cross-DE interoperability matrix | [files.md](files.md) |
| **[Distribution & Packaging](distribution.md)** | Nix binary cache (`aarch64` + `x86_64`), GPG-signed native APT/RPM/Pacman repositories, and AUR publication activation | [distribution.md](distribution.md) |
| **[GPUI & Font Cold-Start](gpui-startup.md)** | Upstream `gpui-fast` hybrid-GPU `D3cold` wake avoidance & single-roundtrip `dmabuf` probe (P1), plus deferred `cosmic-text` / `fontdb` cold-cache criteria (P2.2) | [gpui-startup.md](gpui-startup.md) |
| **[Companion Apps & Platforms](apps.md)** | GDS Nixes type-aware Home Manager editing, suite keyboard shortcut remapping, E2E test expansion, and X11 / macOS / Windows support | [apps.md](apps.md) |
