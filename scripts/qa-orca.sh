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
echo "PASS: Orca emitted speech-dispatch events while keyboard-navigating Retivus and recorded its named controls."
# The static-smoke phase sends dozens of rapid Tab events and floods Orca's
# AT-SPI queue. Start a new reader with a fresh log for the dynamic event
# check, so queued static-focus events cannot mask a live announcement.
kill "$reader" 2>/dev/null || true
wait "$reader" 2>/dev/null || true
reader=0
sleep 1
debug="$RUNNER_TEMP/retivus-orca-dynamic-debug.log"
orca --disable=braille --enable=speech --debug-file="$debug" > "$reader_log" 2>&1 & reader=$!
sleep 3
kill -0 "$reader" || { echo "Fresh Orca did not start" >&2; exit 1; }
xdotool windowactivate --sync "$window"
# Exercise a *dynamic* polite live-region announcement, not only the
# static accessibility label. Default source path is empty in this disposable
# profile, so activating validation must report a safe read failure.
# A historical test searched all recently spoken labels while tabbing in a
# circle; Orca could speak "Validate source" after focus had ALREADY moved to
# the Density combobox. Enter then opened Density instead of validating.
# Establish a known focus location, then make exactly one Tab transition.
xdotool key --clearmodifiers Escape
# Coordinates relative to Retivus's default 800x600 content area, inside the
# unmodified "File" source path text input (not its label or nearby controls).
xdotool mousemove --window "$window" 130 305 click 1
sleep 0.5
focus_start="$(wc -c < "$debug")"
xdotool key --clearmodifiers Tab
# Require a fresh actual AT-SPI FOCUS event, not a stale speech line.
found=0
for attempt in $(seq 1 20); do
  if tail -c "+$((focus_start + 1))" "$debug" |
      grep -F "FOCUS MANAGER: Locus of focus is [button: 'Validate source'" >/dev/null; then
    found=1
    break
  fi
  sleep 0.15
done
if (( !found )); then
  echo 'Orca did not focus Validate source after Tab from File entry' >&2
  scrot "$RUNNER_TEMP/retivus-orca-after-validation.png" || true
  tail -c "+$((focus_start + 1))" "$debug" |
    grep -E 'FOCUS MANAGER: Locus of focus|SPEECH DISPATCHER: Speaking' |
    tail -n 35 >&2 || true
  exit 1
fi
echo 'PASS: actual AT-SPI focus reached Validate source before Return.'
announce_start="$(wc -c < "$debug")"
xdotool key --clearmodifiers Return
sleep 6
# Preserve the visual result to establish that the keyboard action did run.
scrot "$RUNNER_TEMP/retivus-orca-after-validation.png" || true
if ! tail -c "+$((announce_start + 1))" "$debug" \
    | grep -F 'SPEECH DISPATCHER: Speaking' \
    | grep -F 'Could not open source' >/dev/null; then
  echo 'Orca did not speak the changed validation status as a live announcement' >&2
  tail -c "+$((announce_start + 1))" "$debug" \
    | grep -Ei 'SPEECH DISPATCHER|object:announcement|source|name.changed|property' \
    | tail -n 65 >&2 || true
  exit 1
fi
echo 'PASS: real Orca speaks updated validation status after keyboard activation.'

