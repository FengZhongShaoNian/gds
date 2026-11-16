# Hybrid-GPU Direct Vulkan Initialization

On Wayland hybrid-graphics laptops where the compositor renders on an integrated GPU (iGPU) while a discrete GPU (dGPU) sits in runtime power suspension (`runtime_status = suspended`), standard `wgpu` initialization wakes the sleeping dGPU and stalls cold first-frame startup by roughly two seconds. GDS avoids this delay inside its `gpui-fast` fork without changing rendering semantics or leaking process-wide driver filters.

## Problem

In `gpui_wgpu`, `WgpuContext::instance` initializes `wgpu::Instance` with `Backends::VULKAN | Backends::GL` before adapter selection or surface testing runs. Inside `wgpu-hal` (`vulkan/instance.rs`), `desired_extensions(&entry, ...)` queries global Vulkan instance extensions from the Vulkan Loader prior to invoking any creation callback or `vkCreateInstance`. Querying global extensions and layers through the Loader forces it to load and probe every installed Vulkan ICD (including the proprietary NVIDIA driver), waking a suspended dGPU before `enumerate_adapters` or `CompositorGpuHint` sorting ever executes.

Because the wake occurs inside `wgpu::Instance::new`, post-enumeration adapter sorting, `ZED_DEVICE_ID`, `PowerPreference::LowPower`, disabling GL, and filtering EGL are all ineffective.

## Rejected Alternatives

- **Process-wide `VK_DRIVER_FILES` filtering**:
  - Calling `std::env::set_var` in Rust after the Tokio `Supervisor` starts is undefined behavior under multi-threaded Unix environments.
  - Injecting `VK_DRIVER_FILES` in application launchers is inherited by every child process spawned by embedded terminals (`gds-files`, `gds-agents`) and "Open With" external applications, hiding the dGPU from user workloads that legitimately require it.
- **Defaulting to OpenGL (`GL-first`)**:
  - In `wgpu-hal`'s EGL/GLES backend (`src/gles/adapter.rs`, `src/gles/egl.rs`), `surface_capabilities` hardcodes `CompositeAlphaMode::Opaque` (disabling premultiplied alpha used by 10 px Linux client-side rounded corners), presents by performing a full-window Y-flip `glBlitFramebuffer` copy every frame, serializes context access behind a global `Mutex`, restricts swapchain color spaces to sRGB (no wide-gamut or HDR presentation), and reports `device_id = 0` / `DeviceType::Other` (preventing device-ID-based GPU matching).
  - Retaining the primary Vulkan pipeline avoids regressions across GDS applications and preserves color-space headroom for `gds-images`.

## Mechanism & Safety Invariants

The optimization splits **compositor DRM identity propagation** (`gpui_linux`) from **allowlisted exclusive Vulkan initialization and clean two-stage fallback** (`gpui_wgpu`):

1. **Single-Roundtrip Compositor DRM Identity (`gpui_linux`)**:
   - `WaylandClient::new` binds `zwp_linux_dmabuf_v1` (v4) and requests `get_default_feedback` on its existing primary Wayland connection before the initial `event_queue.roundtrip`, avoiding a second socket connection and synchronous roundtrip.
   - `CompositorGpuHint` retains the raw kernel `dev_t` alongside `vendor_id` and `device_id`, and window creation copies rather than consumes (`take()`) the hint so secondary windows and `device_lost` recovery retain the compositor GPU identity.

2. **Precondition Gates (`< 0.5 ms` sysfs + Environment Check)**:
   - Bypass the fast path whenever any explicit GPU, loader, or layer override environment variable is non-empty:
     - `ZED_DEVICE_ID`, `WGPU_BACKEND`, `WGPU_ADAPTER_NAME`
     - `DRI_PRIME`, `__NV_PRIME_RENDER_OFFLOAD`
     - `VK_DRIVER_FILES`, `VK_ADD_DRIVER_FILES`, `VK_ICD_FILENAMES`, `VK_LOADER_DRIVERS_SELECT`, `VK_LOADER_DRIVERS_DISABLE`, `VK_INSTANCE_LAYERS`
   - Resolve `hint.dev_t` (`major:minor`) through `/sys/dev/char/{major}:{minor}/device` to its canonical PCI device path and bound kernel driver symlink (`device/driver`).
   - Scan `/sys/class/drm/card*/device` and enter the fast path **only** when at least one distinct PCI device with a different vendor ID reports `power/runtime_status` in `{"suspended", "suspending"}`. Single-GPU desktops, VMs, and already-active dGPU systems run standard `wgpu` initialization untouched.

3. **Conservative Allowlist ICD Resolution**:
   - Vulkan ICD JSON manifests require only `file_format_version` and `ICD.library_path` (`library_arch` is optional) and carry no hardware PCI IDs or kernel-driver fields, while a single kernel driver can map to multiple user-space ICDs and a single ICD manages all GPUs of that driver (`AMD + AMD` same-ICD topologies cannot be isolated by ICD filtering alone).
   - Therefore, resolution uses a strict allowlist rather than generic manifest guessing:
     - `(0x1002, "amdgpu") -> libvulkan_radeon.so` (Mesa RADV; abort fast path if competing 64-bit `amdvlk` or `amd_icd` manifests are present).
     - `(0x8086, "i915" | "xe") -> libvulkan_intel.so` (Mesa ANV; tolerates Mesa's co-installed `intel_hasvk_icd` manifest).
   - Search `$XDG_CONFIG_HOME/vulkan/icd.d`, `/etc/vulkan/icd.d`, `/run/opengl-driver/share/vulkan/icd.d` (NixOS), `$XDG_DATA_DIRS/vulkan/icd.d`, `/usr/local/share/vulkan/icd.d`, and `/usr/share/vulkan/icd.d`.
   - Reject manifests declaring `library_arch != "64"` (on 64-bit targets), canonicalize matching `library_path` targets, require an exact single unique match, and verify the binary header starts with `\x7fELF` and `EI_CLASS == 2` (64-bit ELF).

4. **4-Step Exclusive Instance Construction (`gpui_wgpu`)**:
   - Load the Vulkan Loader (`ash::Entry::load`) and the verified ICD shared library (`libloading::Library::new`). Never call `enumerate_instance_extension_properties` or `enumerate_instance_layer_properties` on the Loader `entry`, which would scan every ICD.
   - Negotiate the loader–ICD interface version via `vk_icdNegotiateLoaderICDInterfaceVersion` when exported, then resolve `vk_icdGetInstanceProcAddr`.
   - Construct an ICD-direct `driver_entry` (`ash::Entry::from_static_fn`) and invoke `wgpu::hal::vulkan::Instance::desired_extensions(&driver_entry, vk::API_VERSION_1_3, flags)` so extension discovery queries only the selected iGPU driver, then append `VK_LUNARG_direct_driver_loading`.
   - Call `entry.create_instance` with `VkDirectDriverLoadingListLUNARG` in `VK_DIRECT_DRIVER_LOADING_MODE_EXCLUSIVE_LUNARG`, then wrap the raw instance through `wgpu::hal::vulkan::Instance::from_raw` (attaching a `DropCallback` that calls `vkDestroyInstance` before dropping the ICD `Library` and Loader `Entry`) and `wgpu::Instance::from_hal`.

5. **Two-Stage Attempt, Surface Ownership & Recovery**:
   - Always supply explicit `raw_display_handle` in `wgpu::SurfaceTargetUnsafe::RawHandle` so surfaces attach cleanly to `Instance::from_hal` instances (which carry no `InstanceDescriptor::display`).
   - Treat an attempt as successful only when `WgpuContext::new_with_options` completes `create_device`, configures the window `Surface`, and confirms the selected adapter matches `compositor_gpu` (`vendor_id` and `device_id`).
   - Because returning `Result::Err` fallback cannot catch process-level C FFI crashes or hangs, ambiguous driver states abort before `dlopen`. On any returned `Err`, the temporary `wgpu::Surface`, partial device, and exclusive `wgpu::Instance` are dropped completely to release native `wl_surface` ownership before falling back to standard `VULKAN | GL` initialization.
   - Verified `WgpuContext` instances are published to the shared multi-window `GpuContext`, and `WgpuRenderer::recover` reuses the same two-stage construction helper across `device_lost` recovery.

## Retirement Condition

Retire this fast path once upstream `wgpu` ([gfx-rs/wgpu#9912](https://github.com/gfx-rs/wgpu/issues/9912)) and/or Vulkan Loader support targeted DRM-device initialization without probing unrelated suspended GPUs.
