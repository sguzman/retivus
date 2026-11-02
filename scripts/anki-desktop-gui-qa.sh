#!/usr/bin/env bash
# Official Anki Desktop 26.09.3 GUI fixture verification; never uses personal data.
set -euo pipefail
fixture="$1"; root="$2"
mkdir -p "$root"/{home,base,xdg,captures}
export HOME="$root/home" ANKI_BASE="$root/base" XDG_DATA_HOME="$root/xdg"
export QT_QPA_PLATFORM=xcb QTWEBENGINE_DISABLE_SANDBOX=1
export QTWEBENGINE_CHROMIUM_FLAGS="--no-sandbox --disable-gpu"
export LIBGL_ALWAYS_SOFTWARE=1
openbox > "$root/openbox.log" 2>&1 & wm=$!
anki -b "$ANKI_BASE" -l en "$fixture" > "$root/anki.log" 2>&1 & app=$!
capture() { scrot "$root/captures/$1.png" 2>/dev/null || true; }
finish() {
  code=$?
  capture final
  if (( code )); then
    xwininfo -root -tree || true
    tail -n 80 "$root/anki.log" || true
  fi
  kill "$app" "$wm" 2>/dev/null || true
}
trap finish EXIT
for i in $(seq 1 120); do
  window="$(xdotool search --onlyvisible --name Anki 2>/dev/null | head -n 1 || true)"
  [[ -n "$window" ]] && break
  kill -0 "$app" 2>/dev/null || { echo "Anki exited before GUI opened"; exit 1; }
  sleep 1
done
[[ -n "$window" ]] || { echo "Official Anki window not found"; exit 1; }
capture desktop-startup
# Official Anki's first launch asks for a UI language and then requires
# explicit confirmation. The confirmation defaults to No; select the observed
# Yes button directly rather than looping back through the language chooser.
sleep 2
xdotool mousemove --sync 516 396 click 1
sleep 1
xdotool mousemove --sync 779 537 click 1
sleep 3
capture desktop-language-confirmation
xdotool mousemove --sync 743 437 click 1
sleep 5
capture desktop-after-language-confirmation
notes=0; cards=0
for i in $(seq 1 120); do
  db="$(find "$ANKI_BASE" -type f -name collection.anki2 -print -quit 2>/dev/null)"
  if [[ -n "$db" ]]; then
    notes="$(sqlite3 -readonly "$db" 'select count(*) from notes;' 2>/dev/null || echo 0)"
    cards="$(sqlite3 -readonly "$db" 'select count(*) from cards;' 2>/dev/null || echo 0)"
  fi
  [[ "$notes" == 4 && "$cards" == 5 ]] && break
  if [[ "$i" == 8 || "$i" == 20 || "$i" == 40 ]]; then
    capture "import-dialog-$i"
    : # Captures remain available for GUI diagnosis
  fi
  if [[ "$i" == 8 ]]; then
    # Use screenshot-confirmed official Desktop Import button (1280x900 Xvfb).
    xdotool mousemove --sync 957 59 click 1
    sleep 2
    capture desktop-after-import-click
  fi
  sleep 1
done
capture desktop-import-result
[[ "$notes" == 4 && "$cards" == 5 ]] || { echo "Official GUI import did not create 4 notes / 5 cards"; exit 1; }
echo "Official Anki Desktop imported 4 notes / 5 cards"
# Close the completed importer and restore keyboard focus to the main window.
import_window="$(xdotool search --onlyvisible --name "Import File" 2>/dev/null | tail -n 1 || true)"
if [[ -n "$import_window" ]]; then
  xdotool windowclose "$import_window" || true
  sleep 2
fi
main_window="$(xdotool search --onlyvisible --name " - Anki" 2>/dev/null | head -n 1 || true)"
[[ -n "$main_window" ]] || { echo "Main Anki window missing after import"; exit 1; }
xdotool windowactivate --sync "$main_window"
xdotool key --clearmodifiers ctrl+e
sleep 3
capture desktop-export-dialog
dialog="$(xdotool search --onlyvisible --name Export 2>/dev/null | tail -n 1 || true)"
[[ -n "$dialog" ]] || { echo "Official Desktop Export dialog missing"; exit 1; }
xdotool windowactivate --sync "$dialog" || true
xdotool mousemove --window "$dialog" 220 50 click 1 || true
xdotool key --clearmodifiers Home Return
sleep 1
capture desktop-apkg-selected
xdotool key --clearmodifiers Return
sleep 2
capture desktop-save-chooser
xdotool key --clearmodifiers ctrl+a
xdotool type --clearmodifiers --delay 5 "$root/anki-desktop-reexport.apkg"
xdotool key --clearmodifiers Return
for i in $(seq 1 50); do
  [[ -s "$root/anki-desktop-reexport.apkg" ]] && exit 0
  sleep 1
done
echo "Official Desktop .apkg reexport did not appear"; exit 1
