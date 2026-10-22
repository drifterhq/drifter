#!/usr/bin/env bash
set -euo pipefail
umask 022

if [ "$(id -u)" -ne 0 ]; then
    echo "run this as root: sudo $0" >&2
    exit 1
fi

here=$(cd "$(dirname "$0")/.." && pwd)
owly="$here/vendor/owly/packaging"
built="${CARGO_TARGET_DIR:-$here/target}/release/owlyd"

if [ -x /usr/lib/owly/owlyd ]; then
    echo "Owly's radio service is already installed by your package manager; AirDrop uses it"
    exit 0
fi
if [ ! -x "$built" ]; then
    echo "build Drifter first: cargo build --release" >&2
    exit 1
fi

rm -rf /usr/local/lib/drifter
install -D -m 0755 "$built" /usr/local/bin/drifter-owlyd
install -d /etc/systemd/system
sed "s|^ExecStart=/usr/lib/owly/owlyd$|ExecStart=/usr/local/bin/drifter-owlyd|" "$owly/owlyd.service" >/etc/systemd/system/owlyd.service
chmod 0644 /etc/systemd/system/owlyd.service
grep -q '^ExecStart=/usr/local/bin/drifter-owlyd$' /etc/systemd/system/owlyd.service
install -D -m 0644 "$owly/io.github.t4t5.Owly.conf" /etc/dbus-1/system.d/io.github.t4t5.Owly.conf
install -D -m 0644 "$owly/io.github.t4t5.Owly.service" /usr/local/share/dbus-1/system-services/io.github.t4t5.Owly.service
install -D -m 0644 "$owly/io.github.t4t5.Owly.policy" /usr/share/polkit-1/actions/io.github.t4t5.Owly.policy
install -D -m 0644 "$owly/90-owly-unmanaged.conf" /etc/NetworkManager/conf.d/90-owly-unmanaged.conf
install -D -m 0644 "$owly/10-owly-awdl0.link" /etc/systemd/network/10-owly-awdl0.link

if command -v restorecon >/dev/null 2>&1; then
    restorecon -F /usr/local/bin/drifter-owlyd /etc/systemd/system/owlyd.service /etc/dbus-1/system.d/io.github.t4t5.Owly.conf \
        /usr/local/share/dbus-1/system-services/io.github.t4t5.Owly.service /usr/share/polkit-1/actions/io.github.t4t5.Owly.policy \
        /etc/NetworkManager/conf.d/90-owly-unmanaged.conf /etc/systemd/network/10-owly-awdl0.link
fi
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload
    systemctl reload dbus.service 2>/dev/null || busctl call org.freedesktop.DBus / org.freedesktop.DBus ReloadConfig >/dev/null 2>&1 || true
    if [ "$(busctl get-property io.github.t4t5.Owly /io/github/t4t5/Owly io.github.t4t5.Owly.Radio1 State 2>/dev/null)" = 's "idle"' ]; then
        systemctl try-restart owlyd.service || true
    fi
fi
if systemctl is-active --quiet NetworkManager 2>/dev/null; then
    nmcli general reload conf >/dev/null 2>&1 || true
fi
echo "set up AirDrop's radio service"
