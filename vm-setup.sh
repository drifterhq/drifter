#!/usr/bin/env bash
set -euo pipefail

if [ "$(id -u)" -eq 0 ]; then
    echo "run this as dh, not root; it uses sudo where it needs to" >&2
    exit 1
fi

sudo apt-get update

sudo apt-get install -y linux-image-amd64

entry=$(awk -F"'" '/^menuentry / {print $2}' /boot/grub/grub.cfg | grep -v -- '-cloud-' | grep -v recovery | head -1)
if [ -n "$entry" ]; then
    sudo sed -i "s/^GRUB_DEFAULT=.*/GRUB_DEFAULT=\"$entry\"/" /etc/default/grub
    grep -q '^GRUB_DEFAULT=' /etc/default/grub || echo "GRUB_DEFAULT=\"$entry\"" | sudo tee -a /etc/default/grub
    sudo update-grub
fi

sudo apt-get install -y \
    build-essential pkg-config cmake git curl \
    libudev-dev libseat-dev libinput-dev libxkbcommon-dev \
    libgbm-dev libegl1-mesa-dev libwayland-dev wayland-protocols \
    libdisplay-info-dev libdrm-dev libpixman-1-dev \
    seatd foot weston mesa-utils

sudo systemctl enable --now seatd
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

cargo_line='export PATH="$HOME/.cargo/bin:$PATH"'
if ! grep -qF "$cargo_line" "$HOME/.bashrc" 2>/dev/null; then
    printf '%s\n' "$cargo_line" | cat - "$HOME/.bashrc" > "$HOME/.bashrc.new"
    mv "$HOME/.bashrc.new" "$HOME/.bashrc"
fi
printf '%s\n' "$cargo_line" | sudo tee /etc/profile.d/cargo.sh >/dev/null
sudo chmod 0644 /etc/profile.d/cargo.sh

echo
echo "done. reboot to pick up the generic kernel, then check:"
echo "  ls /dev/dri            expect card0 and renderD128"
echo "  ssh trixie rustc -V    expect a version, proving PATH works non-interactively"
