#!/usr/bin/env bash
set -euo pipefail

if [ "$(id -u)" -eq 0 ]; then
    echo "run this as a normal user with sudo; makepkg will not run as root" >&2
    exit 1
fi

here=$(cd "$(dirname "$0")/.." && pwd)
commit=$(git -C "$here" rev-parse HEAD)
work=$here/target/arch
rm -rf "$work"
install -d "$work/build"
cp "$here/dist/arch/PKGBUILD" "$here/dist/arch/drifter-login.install" "$work/build/"
sed -i "/^source=(\"drifter::git+/c\\source=(\"drifter::git+file://$here#commit=$commit\")" "$work/build/PKGBUILD"
cd "$work/build"
makepkg --syncdeps --noconfirm
mv ./*.pkg.tar.zst "$work/"
ls "$work"/*.pkg.tar.zst
