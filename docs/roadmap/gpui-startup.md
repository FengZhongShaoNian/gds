# GPUI & Font Cold-Start Optimization Backlog

In-repo P0 startup optimizations (pure-Rust GNOME theme reader, deferred Settings Center initialization, deduplicated theme reloads, cached Fontconfig alias resolution, and deferred `FileManager1` bus registration) reduced warm-cache `gds-files --release` time-to-first-frame to **313.4 ms** median (`54.5 ms` pre-surface; `275.5 ms` GPU adapter init on Ryzen 7 8745H + RTX 4060 / Weston).

The remaining cold-start bottlenecks live in upstream dependencies and are tracked below.

## P1: `gpui-fast` Hybrid-GPU `D3cold` Wake & Duplicate Wayland Probe

**Target repositories**: `GanDesktop/gpui-fast` and `GanDesktop/gpui-fast-linux`

### Root Causes

1. **Unconditional multi-backend GPU enumeration wakes suspended dGPUs (~600 ms penalty)**:
   - `gpui-fast-wgpu` (`WgpuContext::instance` / `select_adapter_and_device`) creates `wgpu::Instance` with `Backends::VULKAN | Backends::GL` and calls `enumerate_adapters(Backends::all())` before sorting by `CompositorGpuHint`.
   - On hybrid laptops where the Wayland compositor runs on the iGPU and the discrete NVIDIA GPU is suspended in PCIe `D3cold`, enumerating every Vulkan ICD and EGL device node wakes the dGPU even though the iGPU is ultimately chosen.
2. **Duplicate Wayland socket & synchronous roundtrip during startup**:
   - `gpui-fast-linux` (`WaylandClient::new` → `detect_compositor_gpu()`) opens a second `Connection::connect_to_env()`, binds `zwp_linux_dmabuf_v1`, runs a blocking `event_queue.roundtrip()` solely to read `main_device` (`vendor_id` / `device_id`), and immediately closes that connection.

### Planned Upstream Changes

1. **Fold `zwp_linux_dmabuf_v1` feedback into the primary Wayland handshake**: Bind `zwp_linux_dmabuf_v1` during `WaylandClient::new`'s initial registry roundtrip so `CompositorGpuHint` is obtained with zero extra sockets or blocking roundtrips.
2. **Two-stage targeted Vulkan adapter selection**: When `ZED_DEVICE_ID` is unset and `CompositorGpuHint` identifies the compositor GPU, probe a fast path restricted to Vulkan and the matching `vendor_id`/`device_id` first; fall back to full `VULKAN | GL` enumeration only if no compatible surface adapter is found or device loss occurs.
3. **Acceptance**: Verify `/sys/bus/pci/devices/*/power/runtime_status` stays `suspended` on the dGPU throughout `gds-files` cold startup on an iGPU-composited Wayland session without regressing GL/software fallbacks or multi-window `GpuContext` sharing.

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
