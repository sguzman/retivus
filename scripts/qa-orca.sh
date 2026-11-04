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
# The retained screenshots from CI 41871061180 show this native window
# places Validate source around x=8..105, y=355..374. The previous y=331
# clicked the File input: both screenshots still said "No source checked".
# Click the actual button, then *independently verify* the changed status
# through AT-SPI before attributing any failure to Orca's speech handling.
xdotool mousemove --window "$window" 70 365 click 1
sleep 6
scrot "$RUNNER_TEMP/retivus-orca-after-validation.png" || true

# Query the current accessible tree, not Orca's own debug messages, to prove
# the button activation changed the app. Without this check, a mistargeted
# click is incorrectly reported as missing speech. Keep this read-only and
# bounded, using the same standard AT-SPI interfaces as qa-atspi.sh.
address="$(busctl --user call org.a11y.Bus /org/a11y/bus org.a11y.Bus GetAddress | sed -n 's/^s "\\(.*\\)"$/\\1/p')"
[[ -n "$address" ]] || { echo 'No AT-SPI bus available for validation status check' >&2; exit 1; }
status_found=0
status_nodes=0
scan_validation_status() {
  local service="$1" object="$2" depth="$3" name children child_service child_path
  (( depth <= 12 && status_nodes < 250 )) || return 0
  status_nodes=$((status_nodes + 1))
  name="$(busctl --address="$address" --timeout=2s get-property "$service" "$object" org.a11y.atspi.Accessible Name 2>/dev/null || true)"
  if [[ "$name" == *'Could not open source:'* ]]; then
    status_found=1
    return 0
  fi
  children="$(busctl --address="$address" --timeout=2s call "$service" "$object" org.a11y.atspi.Accessible GetChildren 2>/dev/null || true)"
  while IFS=# the emitted AT-SPI announcement AND its observed Orca speech-dispatch call.
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
\\t' read -r child_service child_path; do
    [[ -n "$child_service" && -n "$child_path" ]] || continue
    scan_validation_status "$child_service" "$child_path" "$((depth + 1))"
    (( status_found )) && return 0
  done < <(printf '%s\\n' "$children" | grep -oE '"[^"]+"' | tr -d '"' | paste - -)
}
children="$(busctl --address="$address" --timeout=2s call org.a11y.atspi.Registry /org/a11y/atspi/accessible/root org.a11y.atspi.Accessible GetChildren)"
while IFS=# the emitted AT-SPI announcement AND its observed Orca speech-dispatch call.
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
\\t' read -r service object; do
  [[ -n "$service" && -n "$object" ]] || continue
  scan_validation_status "$service" "$object" 0
  (( status_found )) && break
done < <(printf '%s\\n' "$children" | grep -oE '"[^"]+"' | tr -d '"' | paste - -)
if (( ! status_found )); then
  echo 'Validation control did not produce changed accessible status; inspect retained screenshot before diagnosing Orca' >&2
  exit 1
fi
echo "PASS: direct activation updated the independent AT-SPI validation status ($status_nodes nodes inspected)."
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
