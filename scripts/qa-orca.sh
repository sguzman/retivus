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
# The AT-SPI status event sometimes arrives while Orca still reports focus
# on the window frame. A fixed delay does not establish that the native
# reader is tracking a real widget. Set focus to File and require actual Orca
# speech *before* testing the unrelated changing-status live region.
xdotool mousemove --window "$window" 130 305 click 1
file_ready=0
for attempt in $(seq 1 50); do
  if grep -F "SPEECH DISPATCHER: Speaking 'File'" "$debug" >/dev/null; then
    file_ready=1
    break
  fi
  kill -0 "$reader" 2>/dev/null || { echo "Orca exited before File focus" >&2; exit 1; }
  sleep 0.2
done
if (( ! file_ready )); then
  echo 'Orca did not speak File after explicit widget focus; no dynamic status tested' >&2
  exit 1
fi
# Allow the reader to finish queued focus events. This does not weaken the
# subsequent object:announcement + spoken status assertion.
sleep 3
announce_start="$(wc -c < "$debug")"
# The 800x600 native content frame places Validate source at x=8..105,
# y=322..342. Clicking the button is the isolated status-change stimulus.
xdotool mousemove --window "$window" 70 331 click 1
sleep 6
# Preserve the visual result to establish that the keyboard action did run.
scrot "$RUNNER_TEMP/retivus-orca-after-validation.png" || true
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
