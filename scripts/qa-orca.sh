#!/usr/bin/env bash
# Orca test-only regression: synthetic app window, disposable session and home.
set -euo pipefail
app="$(realpath "$1")"
test -x "$app"
mode="${2:?Usage: qa-orca.sh RETIVUS_BINARY static|dynamic}"
case "$mode" in static|dynamic) ;; *) echo "Unknown Orca test mode: $mode" >&2; exit 2;; esac
export HOME="$RUNNER_TEMP/retivus-orca-home-$mode"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"
debug="$RUNNER_TEMP/retivus-orca-$mode-debug.log"
app_log="$RUNNER_TEMP/retivus-orca-$mode-app.log"
reader_log="$RUNNER_TEMP/retivus-orca-$mode-startup.log"
wm_log="$RUNNER_TEMP/retivus-orca-$mode-wm.log"
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
  if (( reader > 0 )); then
    # Orca is a single-instance screen reader. A terminated launcher can leave
    # its actual daemon registered, breaking the next independent QA session.
    orca --quit >/dev/null 2>&1 || true
    kill "$reader" 2>/dev/null || true
  fi
  if (( wm > 0 )); then kill "$wm" 2>/dev/null || true; fi
}
trap cleanup EXIT
openbox > "$wm_log" 2>&1 & wm=$!
orca --replace --disable=braille --enable=speech --debug-file="$debug" > "$reader_log" 2>&1 & reader=$!
sleep 3
if ! kill -0 "$reader" 2>/dev/null; then
  echo 'Orca launcher failed before Retivus started' >&2
  cat "$reader_log" >&2 || true
  exit 1
fi
if grep -Fq 'Another screen reader process is already running' "$reader_log"; then
  echo 'Orca singleton collision: replacement failed' >&2
  cat "$reader_log" >&2
  exit 1
fi
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
if [[ "$mode" == static ]]; then
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
  exit 0
fi
# Dynamic announcement uses an independent GUI/DBus session. Native keyboard
# activation is already tested by kittest; direct pointer activation here
# isolates live AT-SPI delivery from Orca focus timing.
ready=0
for attempt in $(seq 1 60); do
  if grep -Fq 'SPEECH DISPATCHER: Speaking' "$debug"; then
    ready=1
    break
  fi
  sleep 0.2
done
(( ready )) || { echo "Orca did not initialize speech before dynamic test" >&2; exit 1; }
announce_start="$(wc -c < "$debug")"
# The 800x600 native content frame places Validate source at x=8..105,
# y=322..342. Clicking the button is the isolated status-change stimulus.
xdotool mousemove --window "$window" 70 331 click 1
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
echo 'PASS: Orca speaks the changed validation status after direct button activation.'

