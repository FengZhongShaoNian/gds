# Distribution & Packaging Roadmap

Current releases publish prebuilt Debian/Ubuntu `.deb`, Fedora `.rpm`, and Arch `.pkg.tar.zst` artifacts to GitHub Releases and Cloudflare R2 (`https://pkgs.gandesktop.dev`), installed via `https://gds.gandesktop.dev/install.sh` alongside source-evaluated Nix flake and NUR targets.

## P1: Prebuilt Nix Binary Cache & `aarch64-linux` CI

- **Nix binary cache**: Publish signed Nix store paths from [release.yml](../../.github/workflows/release.yml) to an R2-backed binary cache (`cache.gandesktop.dev`) or Cachix, declaring `nixConfig.extra-substituters` and `extra-trusted-public-keys` in `flake.nix` so `nix profile add github:GanDesktop/gds#gds-files`, NixOS rebuilds, and NUR users download prebuilt closures instead of compiling GPUI locally.
- **`aarch64-linux` release matrix**: Add native ARM64 Linux runners to build `.deb`, `.rpm`, `.pkg.tar.zst`, and Nix store closures alongside `x86_64-linux`.
- **NUR & AUR first-time activation**:
  - Submit `packaging/nix/nur-registration.json` to [`nix-community/NUR`](https://github.com/nix-community/NUR) and set `NUR_REPOSITORY=gandesktop` in GitHub Actions variables (see [NixOS & NUR](../nixos.md#nur-releases)).
  - Once AUR account registration is configured, set `AUR_SSH_PRIVATE_KEY`, `AUR_KNOWN_HOSTS`, and `AUR_ENABLED=true` so `release.yml` automatically pushes `PKGBUILD` + `.SRCINFO` on stable tags (see [Arch & AUR](../apps/files/arch.md#release-automation)).

## P2: Native GPG-Signed Package Repositories

Generate static repository metadata on Cloudflare R2 (`pkgs.gandesktop.dev`) during `release.yml` `publish` and sign metadata with a dedicated offline-master / CI-subkey GPG setup so standard system upgrades (`apt upgrade`, `dnf upgrade`, `pacman -Syu`) update GDS automatically:

- **APT repository** (`dists/<suite>/InRelease`, `Packages.gz`) installed via `/etc/apt/sources.list.d/gds.sources` and `/usr/share/keyrings/gds-archive-keyring.gpg`.
- **DNF / RPM repository** (`repodata/repomd.xml` + `repomd.xml.asc`) configured via `/etc/yum.repos.d/gds.repo`.
- **Pacman binary repository** (`gds.db.tar.zst` + package signatures) for zero-compile Arch/CachyOS upgrades.
