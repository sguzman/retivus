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
# Query the standard per-object Accessible interface, not Cache.GetItems:
# some AT-SPI implementations do not expose a populated bulk cache.
found_file=0
found_validate=0
nodes=0
scan_tree() {
  local service="$1" object="$2" depth="$3" name children child_service child_path
  (( depth <= 12 && nodes < 250 )) || return 0
  nodes=$((nodes + 1))
  name="$(busctl --address="$address" get-property "$service" "$object" org.a11y.atspi.Accessible Name 2>/dev/null || true)"
  if [[ "$name" == *'"File"'* ]]; then found_file=1; fi
  if [[ "$name" == *'"Validate source"'* ]]; then found_validate=1; fi
  if (( found_file && found_validate )); then return 0; fi
  children="$(busctl --address="$address" call "$service" "$object" org.a11y.atspi.Accessible GetChildren 2>/dev/null || true)"
  while IFS=$'\t' read -r child_service child_path; do
    [[ -n "$child_service" && -n "$child_path" ]] || continue
    scan_tree "$child_service" "$child_path" "$((depth + 1))"
    if (( found_file && found_validate )); then return 0; fi
  done < <(printf '%s\n' "$children" | grep -oE '"[^"]+"' | tr -d '"' | paste - -)
}
for attempt in $(seq 1 35); do
  kill -0 "$app_pid" 2>/dev/null || { echo "Retivus exited during AT-SPI inspection" >&2; exit 1; }
  if xdotool search --onlyvisible --name Retivus >/dev/null 2>&1; then
    # The registry's root returns the application service and object path.
    children="$(busctl --address="$address" call org.a11y.atspi.Registry /org/a11y/atspi/accessible/root org.a11y.atspi.Accessible GetChildren 2>/dev/null || true)"
    while IFS=$'\t' read -r service object; do
      [[ -n "$service" && -n "$object" ]] || continue
      scan_tree "$service" "$object" 0
      if (( found_file && found_validate )); then
        echo "PASS: Real Retivus AT-SPI tree exposes File and Validate source controls ($nodes nodes checked)."
        exit 0
      fi
    done < <(printf '%s\n' "$children" | grep -oE '"[^"]+"' | tr -d '"' | paste - -)
  fi
  sleep 1
done
echo "AT-SPI tree missing required labels: File=$found_file Validate-source=$found_validate, nodes=$nodes" >&2
busctl --address="$address" call org.a11y.atspi.Registry /org/a11y/atspi/accessible/root org.a11y.atspi.Accessible GetChildren || true
exit 1
