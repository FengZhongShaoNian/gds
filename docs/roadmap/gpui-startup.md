# GPUI & Font Cold-Start Optimization Backlog

In-repo P0 startup optimizations (pure-Rust GNOME theme reader, deferred Settings Center initialization, deduplicated theme reloads, cached Fontconfig alias resolution, and deferred `FileManager1` bus registration) reduced warm-cache `gds-files --release` time-to-first-frame to **313.4 ms** median (`54.5 ms` pre-surface; `275.5 ms` GPU adapter init on Ryzen 7 8745H + RTX 4060 / Weston).

P1 has now been implemented in GDS's `gpui-fast` fork; the deferred font-discovery item remains tracked below.

## P1: Hybrid-GPU `suspended` Wake & Duplicate Wayland Probe (Implemented)

Moved to [Hybrid-GPU Direct Vulkan Initialization](../decisions/hybrid-gpu-vulkan-init.md).

Summary of shipped changes in `GanDesktop/gpui-fast` (`gds-0.1.16`):
- Bound `zwp_linux_dmabuf_v1` during `WaylandClient::new`'s initial registry roundtrip and retained `dev_t` in `CompositorGpuHint` across windows and recovery.
- Added a gated `VK_LUNARG_direct_driver_loading` exclusive-driver fast path in `gpui_wgpu` (with direct-ICD extension discovery and clean `VULKAN | GL` fallback) when an allowlisted Mesa iGPU (`RADV` / `ANV`) drives the Wayland compositor alongside a runtime-suspended heterogeneous dGPU.

---

## P2.2: `cosmic-text` / `fontdb` Cold Font Scan & `wght` `mmap` Probes (Deferred)

**Tracking issue**: [GanDesktop/gds#8](https://github.com/GanDesktop/gds/issues/8)

### Root Causes

1. **Cold directory scan (`~1.5 s` without warm OS/Fontconfig cache)**: `cosmic-text 0.19` → `fontdb 0.23` parses `fonts.conf` XML paths and recursively scans font files rather than reading `libfontconfig`'s binary `/var/cache/fontconfig/*.cache-*` index.
2. **First-frame variable-weight `mmap` sweep**: `FontSystem::get_font_matches` invokes `db.with_face_data` + `skrifa::FontRef` across non-matching weights to probe `wght` variable axes on the first frame using non-400 weights.

### Why Deferred & Reopen Criteria

Maintaining a long-lived `cosmic-text` / `fontdb` fork adds high CJK/emoji fallback regression risk and lockfile churn while warm-cache pre-surface startup already sits at `54.5 ms`. Packaging checks (ensuring distro and Nix installs can read host Fontconfig caches) take priority.

Reopen for an upstream `pop-os/cosmic-text` contribution (caching per-face `wght` axis bounds and evaluating `FcFontList` metadata discovery) **only if**:
- Packaged distro installs still regularly hit `> 500 ms` cold font scans in real user sessions, or
- First-frame `get_font_matches` `with_face_data` probes become a measurable pre-interaction bottleneck.
