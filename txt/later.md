# Later

Work that is not yet done, after the 2026-10-01 daily-driver list. Done items leave this file.

## Must, before daily use

- Run Drifter on bare metal: the RTX 5060 box on the proprietary NVIDIA driver. Check smoothness, real input, the touchpad, monitor hotplug and VT switching. Fix what breaks. Nothing here has run outside the VM.
- Screen sharing on pixman. Pixman only binds linear dmabufs. If the portal hands Drifter a GPU-tiled buffer, the copy fails. Test a share with no GPU, and fall back to a shared memory copy when the bind fails.

## Soon

- Tray submenus. A dbusmenu item with `children-display` is drawn but does not open, so nm-applet and Nextcloud are only half usable.
- Night light through wlr gamma control, with a Settings page for the warmth and the hours.
- Clipboard that outlives its app. Today a copy is lost when the app it came from closes.
- The headless backend and frame capture, so tests run without the VM.
- A world-sized wallpaper as mipmapped tiles. Today the whole image is one texture, which costs memory and can pass the GPU's texture size limit.

## Deferred

- Saved camera presets. A coordinate plus a zoom level, bound to a keyboard shortcut and optionally a launcher button.
- Moving windows directly in the overview.
- A subtle grid or coordinate system shown when zoomed out.
