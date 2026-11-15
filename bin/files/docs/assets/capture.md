# Recording README Demo Assets

The screenshots and GIFs in `bin/files/docs/assets/` are captured from a real running `gds-files` instance on Niri / Wayland using an English UI, live DMS system themes, and classic desktop wallpapers—never synthetic mockups.

## Prerequisites

Requires an idle Niri + DMS Wayland session (`>= 1280 × 840` logical resolution) with `/dev/uinput` write access, `gds-files`, `grim`, `wf-recorder`, `ffmpeg`, `wtype`, `wl-paste`, `dbus-run-session`, `fastfetch`, and Python 3.11+ with `evdev`.

From the repository root:

```sh
python3 bin/files/docs/assets/fetch-wallpapers.py "$HOME/.cache/gds-readme-wallpapers"
python3 bin/files/docs/assets/record-demos.py --wallpapers "$HOME/.cache/gds-readme-wallpapers"
```

Wallpaper URLs and licenses are maintained in [theme-sync-sources.md](theme-sync-sources.md). Keep the wallpaper cache inside a directory readable by the desktop session (such as `~/.cache`). The recorder creates an isolated temporary demo directory, English config/state, and private D-Bus session, then restores your original wallpaper and focus upon completion without touching your personal Files windows or settings. Avoid moving focus while `record-demos.py` drives input.

## Recording Individual Scenes

Pass `--scene <name>` (repeatable) and optionally `--output /tmp/preview-dir` to test captures without overwriting committed assets:

```sh
python3 bin/files/docs/assets/record-demos.py \
  --wallpapers "$HOME/.cache/gds-readme-wallpapers" \
  --scene overview --scene picker
```

| Scene | Generated Asset(s) | Captured Workflow |
| --- | --- | --- |
| `overview` | `overview.png` | Image grid, details preview panel, and sidebar |
| `browsing` | `file-browsing.gif` | Folder navigation, view toggle, filtering, and preview (`record-file-browsing.sh` shortcut) |
| `copy-path` | `copy-path.png`, `copy-path.gif` | Context-menu copy path and address-bar paste verification |
| `keyboard` | `keyboard-navigation.gif` | Keyboard tab creation, selection, preview, and tab switching |
| `theme` | `theme-sync.gif` | 16 classic wallpapers + DMS palette sync (1 s per frame) |
| `appearance` | `appearance-settings.png` | Theme, opacity, corners, animations, and font settings |
| `opacity` | `opacity-opaque.png`, `opacity-translucent.png` | Side-by-side opaque vs translucent surface comparison |
| `corners` | `corners-square.png`, `corners-rounded.png` | `400 × 260` top-left crop comparing square and rounded corners |
| `font` | `font-size.gif` | Live UI scaling comparison at `14 px` and `20 px` |
| `animations` | `interface-animations.gif` | Sidebar and details panel transitions |
| `picker` | `file-picker.png` | Multi-select image picker dialog |
| `terminal` | `terminal.png` | File browser alongside `fastfetch` inside the embedded terminal |

Override helper binaries via `INPUT_PYTHON`, `GRIM`, and `WF_RECORDER` when needed.

## Export & Inspection

Standard PNGs capture a `1200 × 760` window framed by `24 px` of desktop margin; motion scenes export real-time `15 fps`, `960 px`-wide GIFs (`edit-file-browsing.py <input.mp4> <output.gif>` re-encodes raw MP4 captures without artificial speedups). Inspect every output frame after recording to confirm thumbnails finished loading and no labels or borders were clipped.
