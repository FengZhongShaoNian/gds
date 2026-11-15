# Install & Build GDS Files on Fedora

GDS Files and GDS Portal can be installed as native Fedora RPMs without Rust or Nix on the target machine. Prebuilt RPMs are published on [GitHub Releases](https://github.com/GanDesktop/gds/releases) and via the installer script in the [Files User Guide](../../../bin/files/README.md).

## Install, Update & Remove

```sh
sudo dnf install ./gds-files-*.rpm
gds-files
```

- Installs `gds-files`, its localized `.desktop` launcher, icon, and shared-library dependencies while preserving your existing default file manager and user settings.
- Upgrade by running `sudo dnf install ./gds-files-*.rpm` with a newer package (local builds carry timestamped RPM release numbers).
- Remove with `sudo dnf remove gds-files` (user preferences in `~/.config/gds/app/files/` and state in `~/.local/state/gds/files/` are preserved).
- Optionally set GDS Files as your user's default folder handler:
  ```sh
  xdg-mime default io.github.GanDesktop.GdsFiles.desktop inode/directory
  ```

## Optional GDS Portal

The build also produces the standalone `xdg-desktop-portal-gds-*.rpm` package for native system Open/Save file dialogs:

```sh
sudo dnf install ./xdg-desktop-portal-gds-*.rpm
```

Installing the RPM does not alter desktop Portal routing automatically. To route `FileChooser` requests to GDS in your active desktop's user configuration (for example `~/.config/xdg-desktop-portal/gnome-portals.conf`), add to `[preferred]` while preserving existing entries:

```ini
[preferred]
org.freedesktop.impl.portal.FileChooser=gds
```

Log out and back in so `xdg-desktop-portal` reloads the configuration (`xdg-desktop-portal-gds` activates on demand via D-Bus and does not need `systemctl enable`). Remove the `FileChooser=gds` line before running `sudo dnf remove xdg-desktop-portal-gds` to restore the desktop's default picker.

## Building RPMs Locally

On a development host with Python 3.11+ and Docker or Podman:

```sh
just rpm                     # Fedora 43, incremental dev profile
just rpm 44 release          # Fedora 44, optimized release profile
python3 scripts/build-rpm.py --fedora 44 --profile dev --engine podman
```

- Compiles the current working tree (including uncommitted changes) inside a Fedora container against Fedora's system libraries (`packaging/fedora/`).
- `dev` reuses incremental caches (`target/fedora-<ver>/`) and fast RPM compression; `release` uses the workspace `[profile.release]` configuration (`opt-level = 3`, `lto = false`, `codegen-units = 16`, stripped symbols).
- Output RPMs are written to `target/rpm/fedora-<ver>/<arch>/`.

For deploying and testing built RPMs inside local libvirt Fedora guests (including `just vm fedora copy`, minimal-dependency checks, and Mesa/SPICE notes), see [Desktop Test Virtual Machines](../../development/virtual-machines.md).
