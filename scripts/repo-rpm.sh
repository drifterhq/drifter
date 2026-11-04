#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
key=$(gpg --batch --list-secret-keys --with-colons | awk -F: '/^fpr/ { print $10; exit }')
if [ -z "$key" ]; then
    echo "gpg has no secret key; import the Drifter signing key first" >&2
    exit 1
fi

release=$(rpm -E %fedora)
arch=$(uname -m)
top=$here/target/repo/rpm
dest=$top/fedora/$release/$arch
rm -rf "$top"
install -d "$dest"
find "$here/target/rpm/RPMS" -name '*.rpm' ! -name '*-debuginfo-*' ! -name '*-debugsource-*' -exec cp {} "$dest/" \;
rpmsign --define "_gpg_name $key" --addsign "$dest"/*.rpm
createrepo_c --quiet "$dest"
gpg --batch --yes --local-user "$key" --armor --detach-sign "$dest/repodata/repomd.xml"
cat >"$top/drifter.repo" <<'REPO'
[drifter]
name=Drifter
baseurl=https://drifterhq.github.io/drifter/rpm/fedora/$releasever/$basearch
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=https://drifterhq.github.io/drifter/drifter.asc
REPO
echo "signed dnf repo for Fedora $release in $dest"
