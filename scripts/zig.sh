#!/usr/bin/env bash
set -euo pipefail

version=0.17.1
case "$(uname -m)" in
x86_64) arch=x86_64 sum=3581863e3cbcfd0fce0efc682ce7e9e93da96668a977791d356a26d2a7e9b17a ;;
aarch64 | arm64) arch=aarch64 sum=a1546d5646eb184437918d6635ca6b762a0e889eac09cef720ee7a1ed3fe7795 ;;
*)
    echo "no Zig $version build for $(uname -m)" >&2
    exit 1
    ;;
esac

dest=${XDG_DATA_HOME:-$HOME/.local/share}/drifter/zig-$version
if [ "$("$dest/zig" version 2>/dev/null)" = "$version" ]; then
    echo "Zig $version is already in $dest"
    exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -fsSL -o "$work/zig.tar.xz" "https://ziglang.org/download/$version/zig-$arch-linux-$version.tar.xz"
echo "$sum  $work/zig.tar.xz" | sha256sum -c --quiet -
tar -xJf "$work/zig.tar.xz" -C "$work"
rm -rf "$dest"
install -d "$(dirname "$dest")"
mv "$work/zig-$arch-linux-$version" "$dest"
echo "installed Zig $version in $dest"
