# Later

Work that is not yet done, after the 2026-10-01 daily-driver list. Done items leave this file.

## Must, before daily use

- Run Drifter on bare metal: the RTX 5060 box on the proprietary NVIDIA driver. Check smoothness, real input, the touchpad, monitor hotplug, VT switching and the night light, which the VM cannot show. Fix what breaks. Nothing here has run outside the VM.
- Screen sharing on pixman. Pixman only binds linear dmabufs. If the portal hands Drifter a GPU-tiled buffer, the copy fails. Test a share with no GPU, and fall back to a shared memory copy when the bind fails.

## Soon

- More headless checks: world bounds, each wallpaper mode, window chrome, drawers. The harness is in `render/tests.rs`.

## Deferred

- Saved camera presets. A coordinate plus a zoom level, bound to a keyboard shortcut and optionally a launcher button.
- Moving windows directly in the overview.
- A subtle grid or coordinate system shown when zoomed out.
