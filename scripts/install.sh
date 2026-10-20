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
@include common-account'
elif [ -f /etc/pam.d/system-auth ]; then
    stack='#%PAM-1.0
auth     include  system-auth
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
for theme in drifter-icons-dark drifter-icons-light; do
    ln -sfn "$here/icons/$theme" "$icons/$theme"
done
echo "linked the Drifter icon themes into $icons"

if [ ! -x "$here/target/release/drifter" ]; then
    echo "build it before you log in: cargo build --release"
fi
