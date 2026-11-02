#!/usr/bin/env bash
# Real Linux AT-SPI accessibility smoke. No user profiles or Python runtime.
set -euo pipefail
app="$(realpath "$1")"
test -x "$app"
log="$RUNNER_TEMP/retivus-atspi-app.log"
wm_log="$RUNNER_TEMP/retivus-atspi-wm.log"
app_pid=0
wm_pid=0
cleanup() {
  status=$?
  if (( status != 0 )); then
    echo "AT-SPI accessibility smoke failed; app log:" >&2
    tail -n 80 "$log" >&2 || true
    tail -n 50 "$wm_log" >&2 || true
  fi
  if (( app_pid > 0 )); then kill "$app_pid" 2>/dev/null || true; fi
  if (( wm_pid > 0 )); then kill "$wm_pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
address="$(busctl --user call org.a11y.Bus /org/a11y/bus org.a11y.Bus GetAddress | sed -n 's/^s "\(.*\)"$/\1/p')"
test -n "$address"
busctl --user set-property org.a11y.Bus /org/a11y/bus org.a11y.Status IsEnabled b true
openbox > "$wm_log" 2>&1 & wm_pid=$!
LIBGL_ALWAYS_SOFTWARE=1 "$app" > "$log" 2>&1 & app_pid=$!
for attempt in $(seq 1 90); do
  if ! kill -0 "$app_pid" 2>/dev/null; then
    echo "Retivus exited before native accessibility registration" >&2
    exit 1
  fi
  if xdotool search --onlyvisible --name Retivus >/dev/null 2>&1; then
    children="$(busctl --address="$address" call org.a11y.atspi.Registry /org/a11y/atspi/accessible/root org.a11y.atspi.Accessible GetChildren 2>/dev/null || true)"
    services="$(printf '%s\n' "$children" | grep -oE ':1\.[0-9]+' | sort -u || true)"
    while read -r service; do
      test -n "$service" || continue
      cache="$(busctl --address="$address" call "$service" /org/a11y/atspi/cache org.a11y.atspi.Cache GetItems 2>/dev/null || true)"
      if [[ "$cache" == *"Retivus"* && "$cache" == *"Validate source"* && "$cache" == *"File"* ]]; then
        echo "PASS: Retivus exposes named File and Validate source controls on the native AT-SPI bus."
        echo "AT-SPI app service: $service"
        exit 0
      fi
    done <<< "$services"
  fi
  sleep 1
done
echo "Retivus did not publish the expected native AT-SPI widget tree" >&2
busctl --address="$address" call org.a11y.atspi.Registry /org/a11y/atspi/accessible/root org.a11y.atspi.Accessible GetChildren || true
exit 1
