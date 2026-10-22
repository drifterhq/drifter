#!/usr/bin/env bash
set -eu
v="virsh -c qemu:///system"
send() { $v qemu-monitor-command trixie "{\"execute\":\"input-send-event\",\"arguments\":{\"events\":[$1]}}" >/dev/null; }
rel() { send "{\"type\":\"rel\",\"data\":{\"axis\":\"$1\",\"value\":$2}}"; }
case "$1" in
  home) for _ in 1 2 3 4 5 6; do rel x -1000; done; for _ in 1 2 3 4; do rel y -1000; done ;;
  move) "$0" home; x=$2; y=$3; while [ "$x" -gt 0 ]; do s=$(( x > 500 ? 500 : x )); rel x $s; x=$((x - s)); done; while [ "$y" -gt 0 ]; do s=$(( y > 500 ? 500 : y )); rel y $s; y=$((y - s)); done ;;
  click) b=${2:-left}; send "{\"type\":\"btn\",\"data\":{\"down\":true,\"button\":\"$b\"}}"; sleep 0.05; send "{\"type\":\"btn\",\"data\":{\"down\":false,\"button\":\"$b\"}}" ;;
  key) shift; $v send-key trixie "$@" >/dev/null ;;
  type) python3 - "$2" <<'PY'
import subprocess, sys
plain = {' ': 'KEY_SPACE', '-': 'KEY_MINUS', '/': 'KEY_SLASH', '.': 'KEY_DOT', '_': None, '~': None}
shifted = {'~': 'KEY_GRAVE', '_': 'KEY_MINUS', '"': 'KEY_APOSTROPHE'}
for ch in sys.argv[1]:
    keys = []
    if ch in shifted:
        keys = ['KEY_LEFTSHIFT', shifted[ch]]
    elif ch.isalpha():
        keys = (['KEY_LEFTSHIFT'] if ch.isupper() else []) + ['KEY_' + ch.upper()]
    elif ch.isdigit():
        keys = ['KEY_' + ch]
    elif plain.get(ch):
        keys = [plain[ch]]
    subprocess.run(['virsh', '-c', 'qemu:///system', 'send-key', 'trixie'] + keys, stdout=subprocess.DEVNULL, check=True)
PY
  ;;
  shot) $v screenshot trixie /tmp/trixie.ppm --screen 0 >/dev/null && python3 -c "
from PIL import Image
Image.open('/tmp/trixie.ppm').save('/tmp/trixie.png')" 2>/dev/null || cp /tmp/trixie.ppm /tmp/trixie.png ;;
esac
