#!/usr/bin/env bash
# Installs the xwayland-satellite that Drifter needs into ~/.local/bin, which
# drifter-session puts first on PATH. No sudo. Distro packages lag: 0.8.2
# gives X focus to override-redirect menus, so Steam and Chromium menus close
# as they open. Build deps: cargo, clang, xcb-util-cursor headers.
set -euo pipefail

tag=v0.8.3
commit=b83eab9
bin=${XDG_BIN_HOME:-$HOME/.local/bin}

cargo=$(command -v cargo || echo "$HOME/.cargo/bin/cargo")
if [ ! -x "$cargo" ]; then
    echo "cargo is missing; install rustup first" >&2
    exit 1
fi

case "$("$bin/xwayland-satellite" -version 2>/dev/null)" in
"${tag#v}" | "$tag" | "$commit"*)
    echo "xwayland-satellite $tag is already in $bin"
    exit 0
    ;;
esac

build=$(mktemp -d)
trap 'rm -rf "$build"' EXIT
"$cargo" install --locked --root "$build" \
    --git https://github.com/Supreeeme/xwayland-satellite --tag "$tag" xwayland-satellite
install -Dm755 "$build/bin/xwayland-satellite" "$bin/xwayland-satellite"
echo "installed xwayland-satellite $("$bin/xwayland-satellite" -version) in $bin"
