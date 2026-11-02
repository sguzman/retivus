#!/usr/bin/env bash
# Orca test-only regression: synthetic app window, disposable session and home.
set -euo pipefail
app="$(realpath "$1")"
test -x "$app"
export HOME="$RUNNER_TEMP/retivus-orca-home"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"
debug="$RUNNER_TEMP/retivus-orca-debug.log"
app_log="$RUNNER_TEMP/retivus-orca-app.log"
reader_log="$RUNNER_TEMP/retivus-orca-startup.log"
wm_log="$RUNNER_TEMP/retivus-orca-wm.log"
wm=0; reader=0; learner=0
cleanup() {
  code=$?
  if (( code )); then
    echo "Orca smoke log excerpt:" >&2
    grep -E -i 'SPEECH DISPATCHER|Retivus|Validate source|File|error' "$debug" | tail -n 50 >&2 || true
    tail -n 35 "$reader_log" >&2 || true
    tail -n 35 "$app_log" >&2 || true
  fi
  if (( learner > 0 )); then kill "$learner" 2>/dev/null || true; fi
  if (( reader > 0 )); then kill "$reader" 2>/dev/null || true; fi
  if (( wm > 0 )); then kill "$wm" 2>/dev/null || true; fi
}
trap cleanup EXIT
openbox > "$wm_log" 2>&1 & wm=$!
orca --disable=braille --enable=speech --debug-file="$debug" > "$reader_log" 2>&1 & reader=$!
sleep 3
LIBGL_ALWAYS_SOFTWARE=1 "$app" > "$app_log" 2>&1 & learner=$!
window=""
for i in $(seq 1 60); do
  window="$(xdotool search --onlyvisible --name Retivus | head -n 1 || true)"
  [[ -n "$window" ]] && break
  kill -0 "$learner" 2>/dev/null || { echo "App exited before opening"; exit 1; }
  sleep 1
done
[[ -n "$window" ]] || { echo "Retivus window not found"; exit 1; }
xdotool windowactivate --sync "$window"
sleep 2
# A real Orca instance must receive the AT-SPI focus events caused by Tab.
for cycle in 1 2; do
  for i in $(seq 1 22); do
    xdotool key --clearmodifiers Tab
    sleep 0.12
  done
  for i in $(seq 1 22); do
    xdotool key --clearmodifiers shift+Tab
    sleep 0.12
  done
done
sleep 4
test -s "$debug" || { echo "Orca produced no debug log"; exit 1; }
grep -F 'SPEECH DISPATCHER: Speaking' "$debug" > "$RUNNER_TEMP/retivus-orca-speech.txt" || true
test -s "$RUNNER_TEMP/retivus-orca-speech.txt" || { echo "Orca did not record speech dispatch"; exit 1; }
grep -F "SPEECH DISPATCHER: Speaking 'Validate source'" "$debug" >/dev/null || { echo "Orca did not announce Validate source"; exit 1; }
grep -F "SPEECH DISPATCHER: Speaking 'File'" "$debug" >/dev/null || { echo "Orca did not announce File"; exit 1; }
for label in Appearance Density Format; do
  grep -F "SPEECH DISPATCHER: Speaking '$label'" "$debug" >/dev/null || { echo "Orca did not announce $label"; exit 1; }
done
# Trigger a real changing live region through a coordinate discovered from the
# AT-SPI object tree. No hard-coded screen coordinates or user profile are used.
address="$(busctl --user call org.a11y.Bus /org/a11y/bus org.a11y.Bus GetAddress | sed -n 's/^s "\\(.*\\)"$/\\1/p')"
test -n "$address"
find_action_center() {
  local service="$1" object="$2" depth="$3" name rect tag x y width height children child_service child_path
  (( depth <= 12 )) || return 1
  name="$(busctl --address="$address" get-property "$service" "$object" org.a11y.atspi.Accessible Name 2>/dev/null || true)"
  if [[ "$name" == *'"Validate source"'* ]]; then
    rect="$(busctl --address="$address" call "$service" "$object" org.a11y.atspi.Component GetExtents u 0 2>/dev/null || true)"
    read -r tag x y width height <<< "$rect"
    if [[ "$x" =~ ^-?[0-9]+$ && "$y" =~ ^-?[0-9]+$ && "$width" =~ ^[0-9]+$ && "$height" =~ ^[0-9]+$ ]] && (( width > 2 && height > 2 )); then
      echo "$((x + width / 2)) $((y + height / 2))"
      return 0
    fi
  fi
  children="$(busctl --address="$address" call "$service" "$object" org.a11y.atspi.Accessible GetChildren 2>/dev/null || true)"
  while IFS=$'\t' read -r child_service child_path; do
    [[ -n "$child_service" && -n "$child_path" ]] || continue
    if find_action_center "$child_service" "$child_path" "$((depth + 1))"; then
      return 0
    fi
  done < <(printf '%s\n' "$children" | grep -oE '"[^"]+"' | tr -d '"' | paste - -)
  return 1
}
roots="$(busctl --address="$address" call org.a11y.atspi.Registry /org/a11y/atspi/accessible/root org.a11y.atspi.Accessible GetChildren)"
center=""
while IFS=$'\t' read -r service object; do
  [[ -n "$service" && -n "$object" ]] || continue
  if center="$(find_action_center "$service" "$object" 0)"; then
    break
  fi
done < <(printf '%s\n' "$roots" | grep -oE '"[^"]+"' | tr -d '"' | paste - -)
[[ -n "$center" ]] || { echo "Could not locate visible Validate source bounds on the real AT-SPI tree" >&2; exit 1; }
read -r click_x click_y <<< "$center"
xdotool mousemove "$click_x" "$click_y" click 1
sleep 4
# Empty source path intentionally fails. Verify the new status is actually
# spoken, rather than merely exposing a passive AccessKit node.
grep -E 'SPEECH DISPATCHER: Speaking .*Could not open source' "$debug" >/dev/null || {
  echo "Orca did not speak the changing source-validation status" >&2
  exit 1
}
echo "PASS: Orca announced named controls and changing validation status through native AT-SPI."
