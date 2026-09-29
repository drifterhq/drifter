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

# Stop the session first, so it does not start the old Drifter again.
pkill -f scripts/drifter-session 2>/dev/null || true
pkill -x drifter 2>/dev/null || true

# The old one must let go of the screen before the new one can take it.
for _ in $(seq 50); do
    pgrep -x drifter >/dev/null || break
    sleep 0.1
done
if pgrep -x drifter >/dev/null; then
    pkill -KILL -x drifter 2>/dev/null || true
    sleep 0.5
fi

nohup sh -c "exec <$tty >$log 2>&1; $here/scripts/drifter-session" >/dev/null 2>&1 &

echo "started, logging to $log"
echo "stop it with: pkill -x drifter"
echo "crashes are logged to ${XDG_STATE_HOME:-$HOME/.local/state}/drifter/crash.log"
