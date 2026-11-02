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
# Use actual keyboard focus instead of traversing every AT-SPI object:
# exhaustive tree polling has caused CI's accessibility bus to stall.
xdotool windowactivate --sync "$window"
focused=0
for attempt in $(seq 1 45); do
  before="$(grep -Fc "SPEECH DISPATCHER: Speaking 'Validate source' as string" "$debug" || true)"
  xdotool key --clearmodifiers Tab
  sleep 0.35
  after="$(grep -Fc "SPEECH DISPATCHER: Speaking 'Validate source' as string" "$debug" || true)"
  if (( after > before )); then
    focused=1
    break
  fi
done
(( focused )) || {
  echo "Orca did not reannounce the keyboard-focused Validate source button" >&2
  exit 1
}
echo "PASS: Orca reannounced the keyboard-focused Validate source button."
xdotool key --clearmodifiers Return
sleep 5
# Empty source path is invalid. Require speech, not just AccessKit metadata.
grep -E 'SPEECH DISPATCHER: Speaking .*Could not open source' "$debug" >/dev/null || {
  echo "Orca did not speak the changing validation error" >&2
  exit 1
}
echo "PASS: Orca announced a changing source-validation status."
