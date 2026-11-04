#!/usr/bin/env bash
# Physical Wayland + Orca acceptance evidence for Retivus M6.4.
# Manual-only: no virtual compositor and no automatic claim of spoken output.
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: bash scripts/qa-physical-wayland-orca.sh /absolute/path/to/retivus [new-evidence-dir]

Run this interactively in a REAL Wayland desktop with Orca enabled. Supply a
debug binary built with --features anki-backend (the visual QA fixtures are
not compiled into release builds). The script launches only disposable,
synthetic review/conflict profiles and saves a private results.tsv and logs.
It does not start/stop Orca, touch your normal Retivus data, take screenshots,
or upload results. You must actually listen for speech; AT-SPI events alone
are not proof. Use p=pass, f=fail, s=skipped for each check.

For each screen, manually resize the Retivus window to the requested
dimensions using your compositor. Test the actual controls before answering.
USAGE
}
if [[ $# -eq 1 && $1 == "--help" ]]; then usage; exit 0; fi
if [[ $# -lt 1 || $# -gt 2 ]]; then usage >&2; exit 2; fi
if [[ ! -t 0 ]]; then
  echo 'Physical accessibility acceptance requires an interactive terminal.' >&2
  exit 2
fi
binary="$(realpath "$1")"
if [[ ! -f "$binary" || ! -x "$binary" ]]; then
  echo "Retivus binary is not executable: $binary" >&2
  exit 2
fi
if [[ ! -v XDG_SESSION_TYPE || "$XDG_SESSION_TYPE" != "wayland" ||
      ! -v WAYLAND_DISPLAY || ! -v XDG_RUNTIME_DIR ]]; then
  echo 'Use a physical native Wayland desktop with WAYLAND_DISPLAY set.' >&2
  exit 2
fi
socket="$WAYLAND_DISPLAY"
if [[ "$socket" != /* ]]; then socket="$XDG_RUNTIME_DIR/$socket"; fi
if [[ ! -S "$socket" ]]; then
  echo "Native Wayland socket not available: $socket" >&2
  exit 2
fi
if ! command -v orca >/dev/null 2>&1; then
  echo 'Orca is required. Install and start Orca yourself before manual QA.' >&2
  exit 2
fi
# An installed Orca binary is not evidence that a screen reader is running.
# Check for either a named Orca process or an interpreter launching Orca.
# This preflight is necessary, not sufficient: the human must also hear it.
if ! pgrep -u "$(id -u)" -x orca >/dev/null 2>&1 &&
   ! pgrep -u "$(id -u)" -f '(^|[[:space:]])(/[^[:space:]]*/)?orca([[:space:]]|$)' >/dev/null 2>&1; then
  echo 'No running Orca screen reader was found for this user. Start Orca in the physical desktop first.' >&2
  exit 2
fi
umask 077
if [[ $# -eq 2 ]]; then
  if [[ -e "$2" || -L "$2" ]]; then
    echo 'Evidence directory must not already exist (no clobber).' >&2
    exit 2
  fi
  mkdir -m 700 "$2"
  evidence="$(realpath "$2")"
else
  evidence="$(mktemp -d /tmp/retivus-physical-qa.XXXXXXXX)"
fi
results="$evidence/results.tsv"
printf 'scenario\tcheck\tresult\n' > "$results"
current_app=0
finished=0
cleanup() {
  if (( current_app > 0 )); then
    kill "$current_app" 2>/dev/null || true
    wait "$current_app" 2>/dev/null || true
  fi
  # Interrupted or failed fixtures must never leave a seemingly complete
  # success transcript, especially if all previous scenarios were passed.
  if (( ! finished )); then
    printf 'Session interrupted or terminated before all scenarios completed: %s\n' "$(date -u +%FT%TZ)" >> "$evidence/session.txt"
    printf 'INCOMPLETE: session interrupted or fixture failed\n' > "$evidence/STATUS.txt"
  fi
}
trap cleanup EXIT
any_incomplete=0

record() {
  local scenario="$1" check="$2" answer
  while true; do
    read -r -p "$check [p/f/s]: " answer
    case "$answer" in
      p|f|s) break ;;
      *) echo 'Enter p (personally verified), f (failed), or s (not tested).' ;;
    esac
  done
  printf '%s\t%s\t%s\n' "$scenario" "$check" "$answer" >> "$results"
  if [[ "$answer" != p ]]; then any_incomplete=1; fi
}

printf 'Evidence: %s\n' "$evidence"
printf 'UTC start: %s\n' "$(date -u +%FT%TZ)" > "$evidence/session.txt"
printf 'Wayland socket: available (real desktop)\n' >> "$evidence/session.txt"
printf 'Orca version:\n' >> "$evidence/session.txt"
orca --version >> "$evidence/session.txt" 2>&1 || true
# Record the exact binary whose behavior a human claims to have observed.
# A pass without verifiable binary provenance is insufficient evidence.
printf 'Retivus binary SHA-256: %s\n' "$(sha256sum "$binary" | cut -d ' ' -f 1)" >> "$evidence/session.txt"
printf 'Session type: %s\n' "$XDG_SESSION_TYPE" >> "$evidence/session.txt"
echo 'Orca must already be running. Listen to actual speech throughout the test.'
echo 'Do not open a real AnkiWeb account or use real learner data in this QA session.'
read -r -p "Verify you can HEAR Orca speak in this physical desktop; type HEARD to continue: " spoken
if [[ "$spoken" != HEARD ]]; then
  echo 'Aborting without accessibility acceptance: Orca speech has not been personally verified.' >&2
  exit 1
fi
printf 'Orca audible in physical desktop: manually confirmed\n' >> "$evidence/session.txt"

for scene in review conflict; do
  for dimensions in 720x650 1080x800; do
    scenario="$scene-$dimensions"
    root="$evidence/$scenario"
    mkdir -m 700 "$root" "$root/home" "$root/tmp" "$root/config" "$root/data"
    log="$root/retivus.log"
    HOME="$root/home" XDG_CONFIG_HOME="$root/config" XDG_DATA_HOME="$root/data" \
      TMPDIR="$root/tmp" RETIVUS_VISUAL_QA=isolated-2026 \
      RETIVUS_QA_SCENE="$scene" WINIT_UNIX_BACKEND=wayland \
      "$binary" > "$log" 2>&1 &
    current_app=$!
    ready=0
    for attempt in $(seq 1 120); do
      if grep -Fq "Retivus disposable visual QA scene initialized: $scene" "$log"; then
        ready=1; break
      fi
      if ! kill -0 "$current_app" 2>/dev/null; then break; fi
      sleep 0.25
    done
    if (( ready == 0 )); then
      echo "Fixture did not start. Use a debug, Anki-enabled Retivus binary." >&2
      tail -n 25 "$log" >&2 || true
      exit 1
    fi
    echo
    echo "SCENE: $scene / WINDOW: $dimensions"
    echo 'Manually resize the opened Retivus window to this size.'
    echo 'Confirm that Orca is operating in the real desktop, not just running.'
    read -r -p 'Press Enter after sizing and inspecting the window... ' unused
    record "$scenario" 'Populated content visible; no clipped or inaccessible controls'
    record "$scenario" 'High contrast, reduced motion and keyboard focus work'
    if [[ "$scene" == review ]]; then
      echo 'Use the synthetic review controls to grade the due card.'
      record "$scenario" 'Orca actually speaks labels and changed due/review status'
    else
      echo 'Inspect malformed source/conflict UI and activate status-changing controls.'
      record "$scenario" 'Orca actually speaks conflict/validation status changes'
    fi
    read -r -p 'Press Enter to close this disposable scene... ' unused
    kill "$current_app" 2>/dev/null || true
    wait "$current_app" 2>/dev/null || true
    current_app=0
  done
done
printf 'UTC finish: %s\n' "$(date -u +%FT%TZ)" >> "$evidence/session.txt"
echo "Results saved privately in: $results"
finished=1
if (( any_incomplete )); then
  printf 'INCOMPLETE: failed/skipped checks require correction or further QA\n' > "$evidence/STATUS.txt"
  echo 'INCOMPLETE: failed/skipped checks require correction or further QA.' >&2
  exit 1
fi
printf 'MANUALLY REPORTED PASS: pending independent evidence review\n' > "$evidence/STATUS.txt"
echo 'All manually reported checks passed; this is human evidence, not automated Orca speech proof.'
echo 'M6.4 still requires independent review and acceptance of this evidence.'
