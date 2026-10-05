# Later

Work that is not yet done, after the 2026-10-01 daily-driver list. Done items leave this file.

## Must, before daily use

- Run Drifter on bare metal: the RTX 5060 box on the proprietary NVIDIA driver. Check smoothness, real input, the touchpad, monitor hotplug, VT switching and the night light, which the VM cannot show. Fix what breaks. Nothing here has run outside the VM. The card is Blackwell, so it likely needs driver 570 or newer, from trixie backports or NVIDIA's own repo.
- Screen sharing on pixman. Pixman only binds linear dmabufs. If the portal hands Drifter a GPU-tiled buffer, the copy fails. Test a share with no GPU, and fall back to a shared memory copy when the bind fails.
- A `.deb` built with cargo-deb. It installs drifter, drifter-settings and drifter-session, the wayland-sessions entry, /etc/pam.d/drifter, the portal configs, and the art, fonts and default config in /usr/share/drifter. Depends: pipewire, wireplumber, xdg-desktop-portal-wlr, libseat. Recommends: network-manager-gnome, blueman.
- A signed apt repo on GitHub Pages, so `apt install drifter` and `apt upgrade` work.
- Ship xwayland-satellite in the repo if trixie does not have it, so X11 apps work.
- Default login apps. `[login] start` in conf/default.toml is empty. Start `nm-applet --indicator` and `blueman-applet` when they are installed.

## Soon

- WiFi and Bluetooth pages in Settings, over D-Bus to NetworkManager and BlueZ. Then the default config stops starting nm-applet and blueman.
- Read the XDG autostart folders, ~/.config/autostart and /etc/xdg/autostart. Today only the `[login] start` list runs.
- USB disk automount through udisks.

## Settings gaps

From the 2026-10-02 audit. WiFi and Bluetooth are under Soon.

- Brightness under Displays, through the backlight in /sys/class/backlight. Do this first: it is small, and laptops need it.
- Refresh rate under Displays, from the modes each monitor offers.
- Mirror mode under Displays. Today monitors only sit side by side.
- An Accessibility page: large text, high contrast, reduce motion and sticky keys.
- A power profile under Power, saver or performance, through power-profiles-daemon. Battery details there too.
- A Users page to change your password.
- Screenshot options: the save folder and the file format.
- A search box in the sidebar. There are 19 pages.
- Reset all settings, and import or export the config.

## Wayland protocols

- ext-session-lock, so swaylock and other lockers work.
- ext-idle-notify, so swayidle and other idle daemons work.
- foreign-toplevel, so external taskbars and docks see windows.
- keyboard-shortcuts-inhibit, so VMs and remote desktops can take keys like Super.
- virtual-keyboard, for on-screen keyboards and wtype.
- wlr-output-management, for kanshi and wlr-randr.
- linux-drm-syncobj, presentation-time and tearing-control, against stutter in games and video. NVIDIA needs syncobj most.
- xdg-foreign, xdg-dialog, security-context and single-pixel-buffer. Portals and Flatpak use these.

## Reach

- Screen reader support through AT-SPI.
- Translations. All UI text is English in the code.
- HDR and color management.

## Deferred

- Saved camera presets. A coordinate plus a zoom level, bound to a keyboard shortcut and optionally a launcher button.
- Moving windows directly in the overview.
- A subtle grid or coordinate system shown when zoomed out.
