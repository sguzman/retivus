#!/usr/bin/env bash
# Independent GTK app under same Orca/X11/DBus environment as Retivus.
set -euo pipefail
export HOME="$RUNNER_TEMP/retivus-gtk-reference-home"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"
log="$RUNNER_TEMP/retivus-gtk-orca.log"
reader_log="$RUNNER_TEMP/retivus-gtk-reader.log"
wm_log="$RUNNER_TEMP/retivus-gtk-wm.log"
gui_log="$RUNNER_TEMP/retivus-gtk-gui.log"
wm=0; reader=0; gui=0
cleanup() {
  code=$?
  if (( code )); then
    echo 'Independent GTK/Orca speech evidence:' >&2
    grep -E 'SPEECH DISPATCHER: Speaking|FOCUS MANAGER|object:state-changed:focused' "$log" | tail -n 55 >&2 || true
    tail -n 30 "$reader_log" >&2 || true
  fi
  if (( gui > 0 )); then kill "$gui" 2>/dev/null || true; fi
  if (( reader > 0 )); then
    orca --quit >/dev/null 2>&1 || true
    kill "$reader" 2>/dev/null || true
  fi
  if (( wm > 0 )); then kill "$wm" 2>/dev/null || true; fi
}
trap cleanup EXIT
openbox > "$wm_log" 2>&1 & wm=$!
orca --disable=braille --enable=speech --debug-file="$log" > "$reader_log" 2>&1 & reader=$!
sleep 3
kill -0 "$reader" 2>/dev/null || { echo 'Independent Orca reader exited' >&2; exit 1; }
zenity --question --title='Orca GTK reference' \
  --text='Independent screen reader check' \
  --ok-label='Reference OK' --cancel-label='Reference Cancel' > "$gui_log" 2>&1 & gui=$!
window=''
for attempt in $(seq 1 60); do
  window="$(xdotool search --onlyvisible --name 'Orca GTK reference' | head -n 1 || true)"
  [[ -z "$window" ]] || break
  kill -0 "$gui" 2>/dev/null || { echo 'GTK reference window exited' >&2; exit 1; }
  sleep 0.5
done
[[ -n "$window" ]] || { echo 'No visible GTK reference window' >&2; exit 1; }
xdotool windowactivate --sync "$window"
sleep 2
for i in $(seq 1 12); do
  xdotool key --clearmodifiers Tab
  sleep 0.25
done
sleep 3
scrot "$RUNNER_TEMP/retivus-gtk-reference.png" || true
if ! grep -E "SPEECH DISPATCHER: Speaking 'Reference (OK|Cancel)" "$log" >/dev/null; then
  echo 'Independent Orca session did not record speech for named GTK control' >&2
  exit 1
fi
echo 'PASS: real Orca speech dispatch for a native GTK reference control.'
