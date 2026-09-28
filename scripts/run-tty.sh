#!/usr/bin/env bash
set -euo pipefail

tty=${DRIFTER_TTY:-/dev/tty1}
log=/tmp/drifter-tty.log
here=$(cd "$(dirname "$0")/.." && pwd)
bin=$here/target/release/drifter
[ -x "$bin" ] || bin=$here/target/debug/drifter

if [ ! -x "$bin" ]; then
    echo "build it first: cargo build --release" >&2
    exit 1
fi

if [ ! -r "$tty" ]; then
    echo "cannot read $tty" >&2
    echo "drm master needs a real tty; this will not work over ssh without one" >&2
    exit 1
fi

pkill -x drifter 2>/dev/null || true

nohup sh -c "exec <$tty >$log 2>&1; $bin" >/dev/null 2>&1 &

echo "started, logging to $log"
echo "stop it with: pkill -x drifter"
