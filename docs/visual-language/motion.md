# Motion

Motion in GDS clarifies spatial transitions without competing with content. Components reuse GPUI Kit's target-value transitions rather than stacking extra entrance wrappers.

## Reduced-Motion Policy

Files and Agents each expose an independent **Interface animations** preference (`animations_enabled`, default `true`), combined in `src/appearance` with the desktop's reduced-motion setting:

```text
effective_reduced_motion = !animations_enabled || system_reduced_motion
```

- **Platform discovery**: On Linux, GDS subscribes to XDG Settings Portal reduced-motion reads and change signals on the application's Tokio runtime (no polling or per-frame D-Bus queries). On macOS/Windows, it reads GPUI Kit's system snapshot on startup and when re-enabled.
- **Immediate settlement**: When `effective_reduced_motion` is true, in-flight transitions snap immediately to their target and decorative spinner loops stop requesting frames. Functional timers—notification auto-dismiss, file transfers, network heartbeats, terminal output, and media playback—continue unaffected.

## Standard Transitions

All animated transitions use **`ease-out cubic`** without spring overshoot, retarget smoothly from the currently displayed value when reversed mid-flight, and stop scheduling frames once settled:

| Surface / Control | Duration | Behavior |
| --- | --- | --- |
| **Inline text-button keytips** | `140 ms` | Smoothly expands/contracts button width for measured modifier hint (`Ctrl` / `Alt`); reserves zero extra width at rest. |
| **Section chevrons (`DisclosureIcon`)** | `140 ms` | Rotates a right-facing chevron 90° on expand inside fixed-size icon/label slots (`DisclosureIcon::default().id(...).expanded(...)`). |
| **Sidebar width (`SideBar::sample`)** | `160 ms` | Sampled once per frame by the workspace host and shared across the entire left column (see [Sidebar Anatomy](sidebar.md)). |
| **Files panels & footer status** | `160 ms` | Details panel width, terminal dock height (including maximize/restore), sidebar group heights, and compact/expanded footer status badges. |
| **Files selection & grid reflow** | `160 ms` | Tab, row, and grid-tile highlight crossfades; visible grid tiles interpolate bounds by stable file path; switched tabs fade in from `88%` opacity (initial window render is immediate). |

## Never Animated

GDS never animates streaming agent text, virtualized table row heights, terminal character cells, text-input carets, or translucent surface alpha, and never cross-fades duplicate widget trees.
