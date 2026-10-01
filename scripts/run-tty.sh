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

gone() {
    for _ in $(seq "$1"); do
        pgrep -f scripts/drifter-session >/dev/null || pgrep -x drifter >/dev/null || return 0
        sleep 0.1
    done
    return 1
}

pkill -f scripts/drifter-session 2>/dev/null || true
pkill -x drifter 2>/dev/null || true
if ! gone 100; then
    pkill -KILL -f scripts/drifter-session 2>/dev/null || true
    pkill -KILL -x drifter 2>/dev/null || true
    if ! gone 50; then
        echo "the old drifter will not quit; not starting a second one" >&2
        exit 1
    fi
fi
sleep 0.5

setsid -f sh -c 'exec <"$1" >"$2" 2>&1; exec "$3"' drifter-tty "$tty" "$log" "$here/scripts/drifter-session"

echo "started, logging to $log"
echo "stop it with: pkill -x drifter"
echo "crashes are logged to ${XDG_STATE_HOME:-$HOME/.local/state}/drifter/crash.log"
