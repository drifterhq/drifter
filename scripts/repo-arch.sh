#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
key=$(gpg --batch --list-secret-keys --with-colons | awk -F: '/^fpr/ { print $10; exit }')
if [ -z "$key" ]; then
    echo "gpg has no secret key; import the Drifter signing key first" >&2
    exit 1
fi

dest=$here/target/repo/arch/$(uname -m)
rm -rf "$here/target/repo/arch"
install -d "$dest"
cp "$here"/target/arch/*.pkg.tar.zst "$dest/"
for pkg in "$dest"/*.pkg.tar.zst; do
    gpg --batch --yes --local-user "$key" --detach-sign --no-armor "$pkg"
done
repo-add --sign --key "$key" "$dest/drifter.db.tar.gz" "$dest"/*.pkg.tar.zst
for link in drifter.db drifter.db.sig drifter.files drifter.files.sig; do
    if [ -L "$dest/$link" ]; then
        cp --remove-destination "$(readlink -f "$dest/$link")" "$dest/$link"
    fi
done
echo "signed pacman repo in $dest"
