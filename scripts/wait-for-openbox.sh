#!/usr/bin/env bash
# Wait for actual Openbox EWMH ownership before launching a native GUI smoke.
# Never force-map a hidden application window: its normal mapping is what
# the test must prove. Only used by isolated QA with Xvfb/Openbox.
set -euo pipefail
wm_pid="${1:?Usage: wait-for-openbox.sh OPENBOX_PID}"
[[ "$wm_pid" =~ ^[0-9]+$ ]] || { echo "Invalid Openbox PID" >&2; exit 1; }
command -v wmctrl >/dev/null || { echo "wmctrl is required by GUI smoke" >&2; exit 1; }
for attempt in $(seq 1 60); do
  if ! kill -0 "$wm_pid" 2>/dev/null; then
    echo "Openbox process exited before becoming ready" >&2
    exit 1
  fi
  if wmctrl -m 2>/dev/null | grep -Fxq "Name: Openbox"; then
    echo "PASS: Openbox EWMH readiness"
    exit 0
  fi
  sleep 0.2
done
echo "Openbox never claimed EWMH window-manager ownership" >&2
exit 1
