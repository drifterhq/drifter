#!/usr/bin/env bash
set -euo pipefail

if [ "$(id -u)" -eq 0 ]; then
    echo "run this as dh, not root; it uses sudo where it needs to" >&2
    exit 1
fi

sudo apt-get update

sudo apt-get install -y linux-image-amd64

sub=$(grep -oE "gnulinux-advanced-[0-9a-f-]+" /boot/grub/grub.cfg | head -1)
gen=$(grep -oE "gnulinux-[0-9][^']*-advanced-[0-9a-f-]+" /boot/grub/grub.cfg | grep -v -- '-cloud-' | head -1)
if [ -z "$sub" ] || [ -z "$gen" ]; then
    echo "could not find a non-cloud kernel entry in /boot/grub/grub.cfg" >&2
    echo "without the generic kernel there is no /dev/dri and drifter cannot run" >&2
    exit 1
fi
sudo sed -i "/^GRUB_DEFAULT=/d" /etc/default/grub
echo "GRUB_DEFAULT=\"$sub>$gen\"" | sudo tee -a /etc/default/grub >/dev/null

if ! grep -q 'video=Virtual-1:1920x1080' /etc/default/grub; then
    sudo sed -i 's/^\(GRUB_CMDLINE_LINUX="[^"]*\)"/\1 video=Virtual-1:1920x1080"/' /etc/default/grub
fi

sudo update-grub

sudo apt-get install -y \
    build-essential pkg-config cmake git curl \
    libudev-dev libseat-dev libinput-dev libxkbcommon-dev \
    libgbm-dev libegl1-mesa-dev libwayland-dev wayland-protocols \
    libdisplay-info-dev libdrm-dev libpixman-1-dev libpam0g-dev \
    seatd foot weston mesa-utils

sudo apt-get install -y \
    xwayland libxcb1-dev libxcb-cursor-dev clang \
    grim slurp pipewire pipewire-pulse wireplumber \
    xdg-desktop-portal xdg-desktop-portal-wlr xdg-desktop-portal-gtk \
    polkitd

sudo systemctl enable --now seatd
systemctl --user enable --now pipewire.socket pipewire-pulse.socket wireplumber.service
sudo usermod -aG video,render,input "$USER"

sudo timedatectl set-timezone America/Chicago

for unit in getty@tty1 serial-getty@ttyS0; do
    dir=/etc/systemd/system/${unit}.service.d
    sudo mkdir -p "$dir"
    sudo tee "$dir/autologin.conf" >/dev/null <<AUTOLOGIN
[Service]
ExecStart=
ExecStart=-/sbin/agetty --autologin $USER --noclear %I \$TERM
AUTOLOGIN
done
sudo systemctl daemon-reload

if ! command -v rustup >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --default-toolchain stable --profile default
fi

"$HOME/.cargo/bin/cargo" install --locked \
    --git https://github.com/Supreeeme/xwayland-satellite --tag v0.8.3 xwayland-satellite

cargo_line='export PATH="$HOME/.cargo/bin:$PATH"'
if ! grep -qF "$cargo_line" "$HOME/.bashrc" 2>/dev/null; then
    printf '%s\n' "$cargo_line" | cat - "$HOME/.bashrc" > "$HOME/.bashrc.new"
    mv "$HOME/.bashrc.new" "$HOME/.bashrc"
fi
printf '%s\n' "$cargo_line" | sudo tee /etc/profile.d/cargo.sh >/dev/null
sudo chmod 0644 /etc/profile.d/cargo.sh

echo
echo "done. reboot to pick up the generic kernel and the new groups, then check:"
echo "  ls /dev/dri            expect card0 and renderD128"
echo "  ssh trixie rustc -V    expect a version, proving PATH works non-interactively"
