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
# The official application displayed an Import File dialog for our disposable
# fixture. Activate its verified GUI button, then require Anki's own GUI
# export to pass Retivus's independent field/count/template verifier in CI.
# A live SQLite read is not a reliable gate while Desktop owns the database.
capture desktop-before-import
xdotool mousemove --sync 957 59 click 1
sleep 8
capture desktop-after-import-click
echo "Official Anki Desktop import requested; verifying via actual GUI reexport"
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
# Selecting the first (.apkg) format opens the Qt Save As chooser already.
# The filename field is selected; a second Return would save All Decks.apkg
# to the runner working directory instead of our isolated acceptance path.
capture desktop-save-chooser
xdotool key --clearmodifiers ctrl+a
xdotool type --clearmodifiers --delay 5 "$root/anki-desktop-reexport.apkg"
xdotool key --clearmodifiers Return
sleep 2
capture desktop-export-complete
for i in $(seq 1 50); do
  [[ -s "$root/anki-desktop-reexport.apkg" ]] && exit 0
  sleep 1
done
echo "Official Desktop .apkg reexport did not appear"; exit 1
