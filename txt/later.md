# Later

Work that is not yet done, after the 2026-10-01 daily-driver list. Done items leave this file.

## Must, before daily use

- Run Drifter on bare metal: the RTX 5060 box on the proprietary NVIDIA driver. Check smoothness, real input, the touchpad, monitor hotplug, VT switching and the night light, which the VM cannot show. Fix what breaks. Nothing here has run outside the VM. The card is Blackwell, so it likely needs driver 570 or newer, from trixie backports or NVIDIA's own repo.
- A signed apt repo on GitHub Pages, so `apt install drifter` and `apt upgrade` work. The `.deb` itself is done: `scripts/build-deb.sh`.
- Ship xwayland-satellite in the repo if trixie does not have it, so X11 apps work.

## Check on real hardware

The 2026-10-05 and 2026-10-07 work ran on the VM, with fake radios, and partly on the Pixelbook.

- A real USB stick: it opens and the note opens the folder.
- Sticky keys, reduce motion and large text in daily use.
- Pair a keyboard or a phone that asks for a PIN or a number check.
- Join a real hidden network and a real work or school (802.1X) network.
- Mirror and flip with a real second monitor or projector.
- Adaptive sync on a FreeSync or G-Sync monitor (the bare-metal box).
- Orca speaking aloud while you use apps.

## Gaps found 2026-10-07

- Tearing-control takes the hint, but smithay 0.7 and its main branch have no async page flip, so a game that asks for tearing still waits for vsync. It needs a small patch in smithay's atomic page flip, kept in a fork.
- Orca reads Settings, Activity, Search, the window switcher, the lock screen and notifications. The top bar, the dock, right-click menus and Control Center have no keyboard way in yet, so Orca cannot reach them.
- An ICC profile built from lookup tables (A2B) only loads its calibration curves. Matrix and curve profiles get the full GPU color mapping.

## Reach

- HDR output. smithay 0.7 has no color management protocol, and no HDR screen is here to test on.

## Deferred

- A subtle grid or coordinate system shown when zoomed out.
