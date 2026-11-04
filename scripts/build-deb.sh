#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
cd "$here"
cargo build --release --workspace
cargo deb -p drifter --no-build
cargo deb -p drifter-explorer --no-build
cargo deb -p drifter-login --no-build

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
install -d target/debian

meta=$work/drifter-desktop
install -d "$meta/DEBIAN"
sed "s/@VERSION@/$version/g" dist/drifter-desktop/control >"$meta/DEBIAN/control"
dpkg-deb --root-owner-group --build "$meta" "target/debian/drifter-desktop_${version}_all.deb"

greeter=$work/drifter-greeter
install -d "$greeter/DEBIAN"
sed "s/@VERSION@/$version/g" dist/drifter-greeter/control >"$greeter/DEBIAN/control"
install -m755 dist/drifter-greeter/postinst dist/drifter-greeter/postrm "$greeter/DEBIAN/"
echo /etc/greetd/drifter.toml >"$greeter/DEBIAN/conffiles"
install -Dm644 dist/drifter-greeter/greetd.conf "$greeter/usr/lib/systemd/system/greetd.service.d/drifter-greeter.conf"
install -d "$greeter/etc/greetd"
sed -e 's/@VT@/7/' -e 's/@USER@/_greetd/' dist/drifter-greeter/drifter.toml.in >"$greeter/etc/greetd/drifter.toml"
dpkg-deb --root-owner-group --build "$greeter" "target/debian/drifter-greeter_${version}_all.deb"

satellite=$work/xwayland-satellite
git clone --quiet --depth 1 --branch v0.8.3 https://github.com/Supreeeme/xwayland-satellite "$satellite"
cat >>"$satellite/Cargo.toml" <<'META'

[package.metadata.deb]
maintainer = "donuts-are-good <96031819+donuts-are-good@users.noreply.github.com>"
revision = "0drifter1"
section = "x11"
depends = "$auto, xwayland"
extended-description = "Runs X11 apps in Wayland desktops such as Drifter. Built by Drifter from upstream v0.8.3."
META
VERGEN_GIT_DESCRIBE=v0.8.3 cargo deb --manifest-path "$satellite/Cargo.toml" --output target/debian/
