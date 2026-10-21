#!/usr/bin/env bash
set -euo pipefail

if [ "$(id -u)" -eq 0 ]; then
    echo "run this as yourself, not root; it uses sudo where it needs to" >&2
    exit 1
fi

here=$(cd "$(dirname "$0")/.." && pwd)

sudo dnf install -y --skip-unavailable \
    gcc gcc-c++ pkgconf-pkg-config cmake git clang \
    systemd-devel libseat-devel libinput-devel libxkbcommon-devel \
    mesa-libgbm-devel mesa-libEGL-devel wayland-devel wayland-protocols-devel \
    libdisplay-info-devel libdrm-devel pixman-devel pam-devel \
    libxcb-devel xcb-util-cursor-devel

sudo dnf install -y --skip-unavailable \
    xorg-x11-server-Xwayland xwayland-satellite foot grim slurp wf-recorder \
    pipewire wireplumber xdg-desktop-portal xdg-desktop-portal-wlr xdg-desktop-portal-gtk wlr-randr kanshi \
    network-manager-applet bluez bluez-obexd udisks2 iw accountsservice openssl \
    libnotify pulseaudio-utils

sudo install -Dm644 "$here/conf/drifter-portals.conf" /usr/share/xdg-desktop-portal/drifter-portals.conf
sudo install -Dm644 "$here/conf/xdpw.conf" /etc/xdg/xdg-desktop-portal-wlr/config

"$here/scripts/install.sh"

echo
echo "nvidia_drm modeset (expect Y): $(sudo cat /sys/module/nvidia_drm/parameters/modeset)"
