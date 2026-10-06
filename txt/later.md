# Later

Work that is not yet done, after the 2026-10-01 daily-driver list. Done items leave this file.

## Must, before daily use

- Run Drifter on bare metal: the RTX 5060 box on the proprietary NVIDIA driver. Check smoothness, real input, the touchpad, monitor hotplug, VT switching and the night light, which the VM cannot show. Fix what breaks. Nothing here has run outside the VM. The card is Blackwell, so it likely needs driver 570 or newer, from trixie backports or NVIDIA's own repo.
- A signed apt repo on GitHub Pages, so `apt install drifter` and `apt upgrade` work. The `.deb` itself is done: `scripts/build-deb.sh`.
- Ship xwayland-satellite in the repo if trixie does not have it, so X11 apps work.

## Check on the Pixelbook

The 2026-10-05 night-two work ran only on the VM, with fake radios.

- Wi-Fi page on a real card: join with a password, leave, forget, the switch.
- Bluetooth page with a real device: search, pair, connect, remove.
- Refresh rate and resolution on eDP-1, and wlr-randr and kanshi there.
- A real USB stick: it opens and the note opens the folder.
- Sticky keys, reduce motion and large text in daily use.

## Gaps found 2026-10-05

- Bluetooth pairing that needs a PIN or a passkey, like keyboards and phones, needs an agent. Blueman's applet is one. Settings pairs Just Works devices by itself.
- The Wi-Fi page has no hidden networks and no enterprise (802.1X) networks.
- Pixman writes a screen copy by hand only into a linear or implicit-modifier dmabuf. An explicitly tiled one still fails.
- wlr-output-management changes last until the config is next saved or reloaded. Flipped transforms are refused. Adaptive sync is not offered.
- Settings paints its whole window for each change. On the Pixelbook one paint takes about 43 ms, so scrolling tops out near 23 frames a second. Paint only what changed, or keep the sidebar as a ready image.
- Drifter panics when a print to stderr fails ("failed printing to stderr: Input/output error"). It happened on the Pixelbook at 18:08:51 on 2026-10-05, with no restart from us. Logging must never panic.
- The old volume and brightness buttons are hidden now that Control Center has both sliders. Remove their code: the speaker and sun rects, the gauge panel, and their tips and scroll handling.
- The hot corner frames all windows. With one window that fits at zoom 1 it does not zoom out at all.

## Settings gaps

From the 2026-10-02 audit.

- Mirror mode under Displays. Today monitors only sit side by side.
- A Users page to change your password.

## Wayland protocols

- linux-drm-syncobj, presentation-time and tearing-control, against stutter in games and video. NVIDIA needs syncobj most.
- xdg-foreign, xdg-dialog, security-context and single-pixel-buffer. Portals and Flatpak use these.

## Reach

- Screen reader support through AT-SPI.
- Translations. All UI text is English in the code.
- HDR and color management.

## Deferred

- A subtle grid or coordinate system shown when zoomed out.
