#!/usr/bin/env bash
# Validate a real release archive without Cargo, Rust or a source checkout.
set -euo pipefail
test "$#" -eq 1
dist="$1"
test -d "$dist"
if command -v cargo >/dev/null; then
  echo "Clean-machine acceptance must not run with Cargo installed" >&2
  exit 1
fi
archive="$(find "$dist" -maxdepth 1 -type f -name 'retivus-*-linux-x86_64.tar.zst' -print -quit)"
[[ -n "$archive" ]] || { echo "Missing candidate archive" >&2; exit 1; }
(cd "$dist" && sha256sum --check --status "$(basename "$archive").sha256")
sandbox="$(mktemp -d)"
trap 'rm -rf "$sandbox"' EXIT
tar --zstd -xf "$archive" -C "$sandbox"
root="$(find "$sandbox" -mindepth 1 -maxdepth 1 -type d -name 'retivus-*-linux-x86_64' -print -quit)"
test -n "$root"
test -s "$root/VERSION"
test -x "$root/bin/retivus"
test -x "$root/install.sh"
test -x "$root/uninstall.sh"
test -s "$root/share/doc/retivus/source/Cargo.lock"
test -s "$root/share/doc/retivus/source-deps/anki/LICENSE"
test -s "$root/share/doc/retivus/source-deps/rust-url/LICENSE-MIT"
test -s "$root/share/doc/retivus/SOURCE-PROVENANCE.txt"
grep -Fq 'AnkiWeb sync remains unverified.' "$root/RELEASE-CANDIDATE.txt"
prefix="$sandbox/home/.local"
mkdir -p "$sandbox/home"
export HOME="$sandbox/home"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
mkdir -p "$XDG_DATA_HOME/retivus"
printf '%s\n' 'Private study history must survive' > "$XDG_DATA_HOME/retivus/keep-private.txt"
before="$(sha256sum "$XDG_DATA_HOME/retivus/keep-private.txt" | cut -d' ' -f1)"
"$root/install.sh" "$prefix"
(cd "$prefix" && sha256sum --check --status share/retivus/installed-files.sha256)
"$root/install.sh" "$prefix"
test -x "$prefix/bin/retivus"
test -z "$(find "$prefix/bin" -maxdepth 1 -type f -name '.retivus.*' -print -quit)"
# Real GUI launch, not merely an executable linkability check.
xvfb-run -a bash -c '
  set -euo pipefail
  openbox >/tmp/retivus-container-wm.log 2>&1 & wm=$!
  LIBGL_ALWAYS_SOFTWARE=1 "$1" >/tmp/retivus-container-app.log 2>&1 & app=$!
  trap "kill $app $wm 2>/dev/null || true" EXIT
  for i in $(seq 1 90); do
    # The WM_HINTS 'visible' state varies with window-manager timing in a
    # disposable container. Require an actual mapped X11 window instead.
    for win in $(xdotool search --name Retivus 2>/dev/null || true); do
      if xwininfo -id "$win" 2>/dev/null | grep -q "Map State: IsViewable"; then
        echo "PASS: mapped native Retivus window $win"
        exit 0
      fi
    done
    kill -0 "$app" || {
      echo "Retivus process exited before window was mapped"
      cat /tmp/retivus-container-app.log
      exit 1
    }
    sleep 1
  done
  echo "Retivus process survived but no mapped window appeared"
  echo "--- X11 tree ---"
  xwininfo -root -tree 2>&1 | tail -n 35 || true
  echo "--- Openbox log ---"
  cat /tmp/retivus-container-wm.log || true
  echo "--- Retivus log ---"
  cat /tmp/retivus-container-app.log || true
  exit 1
' bash "$prefix/bin/retivus"
"$root/uninstall.sh" "$prefix"
test ! -e "$prefix/bin/retivus"
test ! -e "$prefix/share/applications/retivus.desktop"
test ! -e "$prefix/share/retivus/installed-files.sha256"
test "$(sha256sum "$XDG_DATA_HOME/retivus/keep-private.txt" | cut -d' ' -f1)" = "$before"
echo "PASS: clean Ubuntu container installation, GUI, reinstall, uninstall and preserved private data."
