#!/usr/bin/env bash
# Delete only verified Retivus-managed files. Never touch learner profiles.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
prefix="$HOME/.local"; if (( $# )); then prefix="$1"; fi
[[ "$prefix" = /* && "$prefix" != "/" && ! -L "$prefix" ]] || {
  echo "Require an absolute non-root, non-symlink install prefix" >&2; exit 1
}
marker="$prefix/share/retivus/installed-sha256"
manifest="$prefix/share/retivus/installed-files.sha256"
bin="$prefix/bin/retivus"
[[ -f "$marker" && ! -L "$marker" && -f "$bin" ]] || { echo "No managed Retivus install found" >&2; exit 1; }
[[ "$(cat "$marker")" == "$(sha256sum "$bin" | cut -d' ' -f1)" ]] || {
  echo "Refusing to delete modified binary" >&2; exit 1
}
if [[ -e "$manifest" || -L "$manifest" ]]; then
  [[ -f "$manifest" && ! -L "$manifest" ]] || { echo "Invalid install manifest" >&2; exit 1; }
  (cd "$prefix" && sha256sum --check --status "$manifest") || {
    echo "Refusing to delete modified files or attribution notices" >&2; exit 1
  }
  rm -- "$bin" "$prefix/share/applications/retivus.desktop" \
    "$prefix/share/doc/retivus/LICENSE" \
    "$prefix/share/doc/retivus/ANKI-ATTRIBUTION.md" \
    "$prefix/share/doc/retivus/MIT-original.txt" \
    "$prefix/share/doc/retivus/README.md" "$manifest" "$marker"
  rmdir "$prefix/share/doc/retivus" 2>/dev/null || true
else
  # Legacy candidate: no manifest or installed notices existed.
  cmp -s "$prefix/share/applications/retivus.desktop" "$here/share/applications/retivus.desktop" || {
    echo "Refusing to delete a modified desktop entry" >&2; exit 1
  }
  rm -- "$bin" "$prefix/share/applications/retivus.desktop" "$marker"
fi
rmdir "$prefix/share/retivus" 2>/dev/null || true
echo "Uninstalled managed Retivus files from $prefix"
