#!/usr/bin/env bash
# Orca test-only regression: synthetic app window, disposable session and home.
set -euo pipefail
app="$(realpath "$1")"
test -x "$app"
export HOME="$RUNNER_TEMP/retivus-orca-home"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"
# Orca is Python-based. Register a test-only, nonfatal thread dump
# on SIGUSR2 so a stalled AT-SPI event can be diagnosed from CI evidence.
# This path is scoped to the screen-reader process, never to Retivus.
diagnostics="$RUNNER_TEMP/retivus-orca-python-diagnostics"
mkdir -p "$diagnostics"
cat > "$diagnostics/sitecustomize.py" <<'PY'
import faulthandler
import signal
import sys

faulthandler.register(signal.SIGUSR2, file=sys.stderr, all_threads=True)
PY
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
PYTHONPATH="$diagnostics${PYTHONPATH:+:$PYTHONPATH}" orca --disable=braille --enable=speech --debug-file="$debug" > "$reader_log" 2>&1 & reader=$!
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
# Dynamic check uses the same Orca and D-Bus session, without
# introducing an unrelated File-focus transition immediately before the
# live-status event. The static File/Validate keyboard checks still run below.
sleep 1
announce_start="$(wc -c < "$debug")"
# In the CI screenshots, the X11 client content begins at screen y=219,
# while Validate source occupies y=555..573. xdotool --window coordinates
# are relative to that client content, not to the decorated frame at y=200.
# Thus the button center is near client (70, 345); prior y=331 and y=365
# hit the gaps above and below it, leaving "No source checked".
xdotool mousemove --window "$window" 70 345 click 1
echo "Validation click: $(xdotool getmouselocation)" >&2
sleep 6
scrot "$RUNNER_TEMP/retivus-orca-after-validation.png" || true
# Require independent application-state evidence before claiming an Orca
# failure. This reads the same AT-SPI bus used by real screen readers; it
# does not weaken the actual speech-dispatch assertion below.
if ! bash scripts/qa-atspi-live-status.sh; then
  echo 'Validation button did not expose changed status; inspect retained screenshot before evaluating Orca speech' >&2
  exit 1
fi
# This fresh home contains exactly one validation activation. Require both
# the emitted AT-SPI announcement AND its observed Orca speech-dispatch call.
# Do not use a byte offset in Orca's asynchronously flushed, multi-MB log:
# the previous offset assertion missed an event that the retained log proves
# was spoken and incorrectly failed an otherwise successful run.
if ! grep -F "EVENT MANAGER: object:announcement for [status bar: 'Could not open source:" "$debug" >/dev/null ||
   ! grep -F "SPEECH DISPATCHER: Speaking 'Could not open source:" "$debug" >/dev/null; then
  echo 'Orca did not speak updated validation status after direct button activation' >&2
  # Capture every Python thread before cleanup. This distinguishes an Orca
  # event-loop stall from a healthy loop that deliberately drops the event.
  # If the process has exited, leave the original failure and logs intact.
  if kill -0 "$reader" 2>/dev/null; then
    echo 'Requesting Orca Python thread dump (SIGUSR2)' >&2
    kill -USR2 "$reader" 2>/dev/null || true
    sleep 1
  fi
  tail -c "+$((announce_start + 1))" "$debug" \
    | grep -Ei 'SPEECH DISPATCHER|object:announcement|source|name.changed|property' \
    | tail -n 65 >&2 || true
  exit 1
fi
echo 'PASS: Orca speaks changed validation status before static navigation.'


# Static accessibility navigation runs after the live announcement,
# so its focus-event burst cannot delay or suppress the first update.
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
