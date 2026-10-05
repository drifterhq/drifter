#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
cd "$here"
cargo build --release --workspace
cargo deb -p drifter --no-build
