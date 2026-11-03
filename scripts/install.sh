#!/usr/bin/env bash
set -euo pipefail

if [ "$(id -u)" -eq 0 ]; then
    echo "run this as yourself, not root; it uses sudo where it needs to" >&2
    exit 1
fi

here=$(cd "$(dirname "$0")/.." && pwd)

if [ -f /etc/pam.d/common-auth ]; then
    stack='#%PAM-1.0
@include common-auth
-auth     optional pam_gnome_keyring.so
@include common-account'
elif [ -f /etc/pam.d/system-auth ]; then
    stack='#%PAM-1.0
auth     include  system-auth
-auth    optional pam_gnome_keyring.so
account  include  system-auth'
else
    echo "found neither /etc/pam.d/common-auth nor /etc/pam.d/system-auth" >&2
    echo "write /etc/pam.d/drifter by hand; until then the lock uses /etc/pam.d/login" >&2
    stack=
fi
if [ -n "$stack" ]; then
    printf '%s\n' "$stack" | sudo tee /etc/pam.d/drifter >/dev/null
    sudo chmod 0644 /etc/pam.d/drifter
    echo "wrote /etc/pam.d/drifter"
fi

sudo install -d /usr/share/wayland-sessions
sudo tee /usr/share/wayland-sessions/drifter.desktop >/dev/null <<DESKTOP
[Desktop Entry]
Name=Drifter
Comment=A desktop that is a camera over one large world
Exec=$here/scripts/drifter-session
Type=Application
DesktopNames=drifter
DESKTOP
sudo chmod 0644 /usr/share/wayland-sessions/drifter.desktop
echo "wrote /usr/share/wayland-sessions/drifter.desktop, which runs $here/scripts/drifter-session"

sudo install -Dm644 "$here/dist/drifter-session.target" /usr/lib/systemd/user/drifter-session.target
command -v systemctl >/dev/null 2>&1 && systemctl --user daemon-reload 2>/dev/null || true
echo "wrote /usr/lib/systemd/user/drifter-session.target, which starts graphical-session.target"

sudo install -d /usr/share/applications
sed "s|^Exec=drifter-explorer|Exec=$here/target/release/drifter-explorer|" "$here/dist/drifter-explorer.desktop" |
    sudo tee /usr/share/applications/drifter-explorer.desktop >/dev/null
sed "s|^Exec=drifter-activity|Exec=$here/target/release/drifter-activity|" "$here/dist/drifter-activity.desktop" |
    sudo tee /usr/share/applications/drifter-activity.desktop >/dev/null
sudo install -m 0644 "$here/dist/drifter-mimeapps.list" /usr/share/applications/drifter-mimeapps.list
sudo chmod 0644 /usr/share/applications/drifter-explorer.desktop /usr/share/applications/drifter-activity.desktop
command -v update-desktop-database >/dev/null 2>&1 && sudo update-desktop-database -q /usr/share/applications || true
echo "wrote Explorer and Activity to /usr/share/applications; folders open in Explorer in Drifter sessions"

icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons"
mkdir -p "$icons"
rm -f "$icons/drifter-icons-dark" "$icons/drifter-icons-light"
ln -sfn "$here/icons/drifter-icons" "$icons/drifter-icons"
echo "linked the Drifter icon theme into $icons"

if command -v systemctl >/dev/null 2>&1 && systemctl list-unit-files bluetooth.service >/dev/null 2>&1 \
    && ! systemctl is-enabled --quiet bluetooth.service; then
    sudo systemctl enable --now bluetooth.service
    echo "turned on the Bluetooth service"
fi
command -v bluetoothd >/dev/null 2>&1 || [ -x /usr/lib/bluetooth/bluetoothd ] || [ -x /usr/libexec/bluetooth/bluetoothd ] \
    || echo "install BlueZ for Bluetooth: bluez and bluez-obexd (Debian, Fedora) or bluez and bluez-utils (Arch)"
command -v pactl >/dev/null 2>&1 \
    || echo "install pactl so Bluetooth headphones can pick their sound: pulseaudio-utils (Debian, Fedora) or libpulse (Arch)"

if command -v greetd >/dev/null 2>&1 && [ -x "$here/target/release/drifter" ]; then
    sudo install -Dm755 "$here/target/release/drifter" /usr/local/libexec/drifter/drifter
    sudo install -Dm644 "$here/conf/default.toml" /usr/share/drifter/conf/default.toml
    sudo install -Dm644 "$here/img/cursor.png" /usr/share/drifter/img/cursor.png
    for dir in shaders fonts img/dock img/window img/users; do
        sudo install -d "/usr/share/drifter/$dir"
        sudo cp -r "$here/$dir/." "/usr/share/drifter/$dir/"
    done
    greeter=_greetd
    getent passwd greetd >/dev/null && greeter=greetd
    vt=$(sed -n 's/^ *vt *= *\([0-9]*\).*/\1/p' /etc/greetd/config.toml 2>/dev/null | head -1)
    sed -e "s/@VT@/${vt:-1}/" -e "s/@USER@/$greeter/" \
        -e 's|^command = "drifter"|command = "/usr/local/libexec/drifter/drifter"|' \
        "$here/dist/drifter-greeter/drifter.toml.in" | sudo tee /etc/greetd/drifter.toml >/dev/null
    sudo install -Dm644 "$here/dist/drifter-greeter/greetd.conf" /etc/systemd/system/greetd.service.d/drifter-greeter.conf
    command -v restorecon >/dev/null 2>&1 && sudo restorecon -R /usr/local/libexec/drifter /usr/share/drifter /etc/greetd || true
    sudo systemctl daemon-reload
    echo "set up the Drifter login screen for greetd; run this again after each build to update it"
    systemctl is-enabled --quiet greetd 2>/dev/null \
        || echo "to use it: sudo systemctl disable gdm sddm lightdm 2>/dev/null; sudo systemctl enable greetd; then reboot"
fi

if [ -x "$here/target/release/owlyd" ]; then
    sudo "$here/scripts/airdrop-service.sh"
fi

if [ ! -x "$here/target/release/drifter" ]; then
    echo "build it before you log in: cargo build --release, then run this again"
fi
