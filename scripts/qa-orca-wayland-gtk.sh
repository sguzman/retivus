#!/usr/bin/env bash
# Diagnostic reference control for M6.4: real GTK4, Orca and native Wayland,
# without Retivus or AccessKit. The same GUI focus-speech assertion as X11.
set -euo pipefail
export HOME="$RUNNER_TEMP/retivus-orca-wayland-home"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
export XDG_RUNTIME_DIR="$RUNNER_TEMP/retivus-orca-wayland-runtime"
export XDG_SESSION_TYPE=wayland
export XDG_CURRENT_DESKTOP=sway
export GDK_BACKEND=wayland
export WLR_BACKENDS=headless
export WLR_HEADLESS_OUTPUTS=1
export WLR_RENDERER=pixman
export WLR_RENDERER_ALLOW_SOFTWARE=1
export WLR_LIBINPUT_NO_DEVICES=1
export LIBGL_ALWAYS_SOFTWARE=1
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
# Ubuntu 24.04 Orca 46 consults DISPLAY through xkbcomp even when
# XDG_SESSION_TYPE=wayland. The workflow supplies a sidecar Xvfb DISPLAY
# *only* for Orca keyboard-map startup. GTK uses GDK_BACKEND=wayland
# and Sway disables Xwayland: tested app pixels are native Wayland.
test -n "${DISPLAY:-}" || { echo 'Missing legacy Orca xkbcomp display' >&2; exit 1; }
unset WAYLAND_DISPLAY SWAYSOCK
printf '%s\n' 'output * resolution 1080x800' 'xwayland disable' \
  'default_border pixel 1' > "$RUNNER_TEMP/retivus-orca-sway.conf"
sway_log="$RUNNER_TEMP/retivus-orca-sway.log"
reader_log="$RUNNER_TEMP/retivus-orca-wayland-reader.log"
debug="$RUNNER_TEMP/retivus-orca-wayland-debug.log"
gui_log="$RUNNER_TEMP/retivus-orca-wayland-gtk.log"
sway_pid=0; reader=0; gui=0
cleanup() {
  code=$?
  if (( code )); then
    echo 'Native Wayland GTK and Orca diagnostic:' >&2
    tail -n 25 "$sway_log" >&2 || true
    tail -n 25 "$reader_log" >&2 || true
    grep -E 'FOCUS MANAGER|SPEECH DISPATCHER: Speaking|object:state-changed:focused|object:window:activate' "$debug" | tail -n 65 >&2 || true
    swaymsg -t get_tree >&2 || true
  fi
  if (( gui > 0 )); then kill "$gui" 2>/dev/null || true; fi
  if (( reader > 0 )); then
    orca --quit >/dev/null 2>&1 || true
    kill "$reader" 2>/dev/null || true
  fi
  if (( sway_pid > 0 )); then kill "$sway_pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
sway -c "$RUNNER_TEMP/retivus-orca-sway.conf" > "$sway_log" 2>&1 & sway_pid=$!
for attempt in $(seq 1 80); do
  for socket in "$XDG_RUNTIME_DIR"/wayland-*; do
    if [[ -S "$socket" ]]; then
      export WAYLAND_DISPLAY="${socket##*/}"
      break 2
    fi
  done
  kill -0 "$sway_pid" 2>/dev/null || { echo 'Sway exited early' >&2; exit 1; }
  sleep 0.25
done
[[ -n "${WAYLAND_DISPLAY:-}" ]] || { echo 'Sway Wayland socket missing' >&2; exit 1; }
for attempt in $(seq 1 60); do
  for socket in "$XDG_RUNTIME_DIR"/sway-ipc.*.sock; do
    if [[ -S "$socket" ]]; then
      export SWAYSOCK="$socket"
      break 2
    fi
  done
  sleep 0.25
done
[[ -n "${SWAYSOCK:-}" ]] || { echo 'Sway IPC socket missing' >&2; exit 1; }
swaymsg -t get_outputs > "$RUNNER_TEMP/retivus-orca-wayland-outputs.json"
zenity --question --title='Orca Wayland reference' \
  --text='Independent native Wayland screen reader control' \
  --ok-label='Reference OK' --cancel-label='Reference Cancel' > "$gui_log" 2>&1 & gui=$!
for attempt in $(seq 1 90); do
  if swaymsg -t get_tree | jq -e '.. | objects | select(.name? == "Orca Wayland reference")' >/dev/null; then
    break
  fi
  kill -0 "$gui" 2>/dev/null || { echo 'GTK reference process exited early' >&2; exit 1; }
  if (( attempt == 90 )); then echo 'Wayland GTK reference window missing' >&2; exit 1; fi
  sleep 0.25
done
swaymsg '[title="Orca Wayland reference"] focus' > /dev/null
sleep 1
orca --disable=braille --enable=speech --debug-file="$debug" > "$reader_log" 2>&1 & reader=$!
sleep 3
kill -0 "$reader" 2>/dev/null || { echo 'Wayland Orca exited early' >&2; exit 1; }
swaymsg '[title="Orca Wayland reference"] focus' >/dev/null
for step in $(seq 1 16); do
  wtype -k Tab
  sleep 0.35
done
sleep 4
grim -o HEADLESS-1 "$RUNNER_TEMP/retivus-orca-wayland-gtk.png"
test -s "$RUNNER_TEMP/retivus-orca-wayland-gtk.png"
test -s "$debug"
# The control must actually be spoken. Focus events or only speaking 'tab'
# do not satisfy this diagnostic.
if ! grep -E "SPEECH DISPATCHER: Speaking 'Reference (OK|Cancel)" "$debug" >/dev/null; then
  echo 'Orca failed to speak a native Wayland GTK control label' >&2
  exit 1
fi
echo 'PASS: actual Orca speech for an independent native Wayland GTK control.'
