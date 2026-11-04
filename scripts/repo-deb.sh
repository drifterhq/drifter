#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
key=$(gpg --batch --list-secret-keys --with-colons | awk -F: '/^fpr/ { print $10; exit }')
if [ -z "$key" ]; then
    echo "gpg has no secret key; import the Drifter signing key first" >&2
    exit 1
fi

suite=$(. /etc/os-release && echo "$VERSION_CODENAME")
arch=$(dpkg --print-architecture)
dest=$here/target/repo/deb
rm -rf "$dest"
install -d "$dest/pool/main" "$dest/dists/$suite/main/binary-$arch"
cp "$here"/target/debian/*.deb "$dest/pool/main/"

cd "$dest"
apt-ftparchive packages pool >"dists/$suite/main/binary-$arch/Packages"
gzip -9kf "dists/$suite/main/binary-$arch/Packages"
apt-ftparchive \
    -o APT::FTPArchive::Release::Origin=Drifter \
    -o APT::FTPArchive::Release::Label=Drifter \
    -o APT::FTPArchive::Release::Suite="$suite" \
    -o APT::FTPArchive::Release::Codename="$suite" \
    -o APT::FTPArchive::Release::Architectures="$arch" \
    -o APT::FTPArchive::Release::Components=main \
    -o APT::FTPArchive::Release::Description="Drifter for Debian $suite" \
    release "dists/$suite" >"$dest/Release.new"
mv "$dest/Release.new" "dists/$suite/Release"
gpg --batch --yes --local-user "$key" --clearsign -o "dists/$suite/InRelease" "dists/$suite/Release"
gpg --batch --yes --local-user "$key" --armor --detach-sign -o "dists/$suite/Release.gpg" "dists/$suite/Release"
echo "signed apt repo for $suite in $dest"
