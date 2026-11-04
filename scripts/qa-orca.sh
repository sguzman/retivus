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
pulse_log="$RUNNER_TEMP/retivus-orca-pulse.log"
wm=0; reader=0; learner=0; pulse_started=0
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
  if (( pulse_started )); then pulseaudio --kill >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT
# CI runners have no physical audio device. Speech Dispatcher's eSpeak NG
# module may block on sound output, preventing Orca from delivering later
# announcements. Provide a real PulseAudio stream target backed by an
# isolated null sink: no simulated AT-SPI or speech events are injected.
# Physical audible and native Wayland acceptance remain a separate gate.
pulseaudio --daemonize=yes --exit-idle-time=-1 --log-target="file:$pulse_log"
pulse_started=1
if ! timeout 8s pactl info >/dev/null; then
  echo 'PulseAudio did not expose a usable CI server' >&2
  exit 1
fi
pactl load-module module-null-sink sink_name=retivus_ci_orca >/dev/null
pactl set-default-sink retivus_ci_orca
export PULSE_SINK=retivus_ci_orca
echo 'CI virtual audio sink active:' >&2
pactl get-default-sink >&2
# A real eSpeak/Speech Dispatcher pipeline must acknowledge completion
# *before* Orca starts. Failure here is an audio-service test-environment
# blocker, not an accessibility pass or Retivus status assertion.
if ! timeout 12s spd-say -w -o espeak-ng 'Retivus CI audio service preflight'; then
  echo 'Speech Dispatcher audio preflight failed before Orca startup' >&2
  exit 1
fi
echo 'PASS: eSpeak NG completed a real Speech Dispatcher audio preflight.' >&2
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
# Controlled focus readiness experiment: the newer Orca stack speaks ordinary
# controls when it has a keyboard-focus context, but not the first live status.
# One real Tab event precedes validation; do not fake a focus or speech event.
# Previous pre-validation focus tests had an incorrectly targeted button click
# and therefore could not establish whether focused context helped.
focus_start="$(wc -c < "$debug")"
xdotool key --clearmodifiers Tab
sleep 3
if tail -c "+$((focus_start + 1))" "$debug" | grep -E "SPEECH DISPATCHER: Speaking|EVENT MANAGER: object:state-changed:focused" >/dev/null; then
  echo 'FOCUS-WARMUP: Orca observed a spoken or focused event before validation.' >&2
else
  echo 'FOCUS-WARMUP: No observed spoken/focused event before validation.' >&2
fi
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
# This fresh home contains exactly one validation activation. The
# upstream Orca debug writer is asynchronous: a 6-second immediate grep
# returned false even though the retained log recorded the speech call at
# event time. Do not falsely fail an actually spoken status by reading the
# debug file before its buffers have flushed. Check the same event/speech
# evidence after navigation and allow a bounded further flush window.
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
static_speech_pass=1
if [[ ! -s "$RUNNER_TEMP/retivus-orca-speech.txt" ]]; then
  echo 'FAIL: Orca did not record any speech dispatch' >&2
  static_speech_pass=0
fi
for label in 'Validate source' File Appearance Density Format; do
  # Newer Orca speaks labels and roles as separate utterances; Orca 46 can
  # speak a single combined label+role utterance. Both are actual Orca speech.
  # Do not accept a mere AT-SPI focus event or arbitrary partial label text.
  case "$label" in
    'Validate source') combined='Validate source push button.' ;;
    File) combined='File entry.' ;;
    Appearance|Density|Format) combined="$label combo box." ;;
  esac
  if ! grep -F "SPEECH DISPATCHER: Speaking '$label'" "$debug" >/dev/null &&
     ! grep -F "SPEECH DISPATCHER: Speaking '$combined'" "$debug" >/dev/null; then
    echo "FAIL: Orca did not speak named control: $label" >&2
    static_speech_pass=0
  fi
done
if (( static_speech_pass )); then
  echo 'PASS: Orca spoke core named controls during keyboard navigation.'
fi
# The status event and actual speech call must both exist. Look for these
# exact messages (never only a visual or AT-SPI status) after allowing
# asynchronous Orca debug logging to catch up. The app sent just one
# synthetic validation event; navigation cannot generate this status text.
status_speech_pass=0
for attempt in 1 2 3 4 5 6 7 8; do
  if grep -F "EVENT MANAGER: object:announcement for [status bar: 'Could not open source:" "$debug" >/dev/null &&
     grep -F "SPEECH DISPATCHER: Speaking 'Could not open source:" "$debug" >/dev/null; then
    status_speech_pass=1
    break
  fi
  sleep 1
done
if (( status_speech_pass )); then
  echo 'PASS: Orca spoke the changed validation status (independently retained speech log).'
else
  echo 'FAIL: Orca did not log actual speech for the changed validation status.' >&2
fi
# Neither result substitutes for the other: both are release-gate evidence.
if (( ! status_speech_pass || ! static_speech_pass )); then
  echo "FAIL: Orca acceptance: dynamic_status=$status_speech_pass static_controls=$static_speech_pass" >&2
  # Probe the disposable Speech Dispatcher only after both independent
  # Orca speech checks have finished. This avoids skewing static-control
  # evidence with extra test-generated audio or SIGUSR2 during navigation. A returned spd-say command is not
  # equivalent to Orca announcing the status; this is diagnostic evidence.
  probe="$RUNNER_TEMP/retivus-speechd-probe.log"
  {
    echo "Speech Dispatcher diagnostic at $(date -u +%FT%TZ)"
    if command -v spd-say >/dev/null 2>&1; then
      for probe_number in 1 2; do
        if timeout 8s spd-say "Retivus diagnostic speech probe $probe_number"; then
          echo "Independent SSIP probe $probe_number: command acknowledged"
        else
          echo "Independent SSIP probe $probe_number: timed out or failed ($?)"
        fi
      done
    else
      echo 'spd-say not installed; independent SSIP probe unavailable'
    fi
    ps -eo pid,stat,wchan:24,args | grep -E '[s]peech-dispatcher|[s]d_espeak-ng|[s]d_openjtalk' || true
  } > "$probe" 2>&1
  cat "$probe" >&2
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
echo 'PASS: Orca spoke both changing status and core named controls.'
