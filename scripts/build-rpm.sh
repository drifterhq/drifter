#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
cd "$here"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
top=$here/target/rpm
rm -rf "$top/SPECS"
install -d "$top/SOURCES" "$top/SPECS"

tree=$(git stash create)
git archive --format=tar.gz --prefix="drifter-$version/" -o "$top/SOURCES/drifter-$version.tar.gz" "${tree:-HEAD}"
sed "s/^Version:.*/Version:        $version/" dist/rpm/drifter.spec >"$top/SPECS/drifter.spec"
cp dist/rpm/xwayland-satellite.spec "$top/SPECS/"

satellite=$top/SOURCES/xwayland-satellite-0.8.3.tar.gz
[ -s "$satellite" ] || curl -fsSL -o "$satellite" \
    https://github.com/Supreeeme/xwayland-satellite/archive/v0.8.3/xwayland-satellite-0.8.3.tar.gz

zig=$top/SOURCES/zig-$(uname -m)-linux-0.17.1.tar.xz
[ -s "$zig" ] || curl -fsSL -o "$zig" "https://ziglang.org/download/0.17.1/$(basename "$zig")"

for spec in drifter xwayland-satellite; do
    rpmbuild --define "_topdir $top" -bb "$top/SPECS/$spec.spec"
done
find "$top/RPMS" -name '*.rpm' -newer "$top/SPECS/drifter.spec" -print
