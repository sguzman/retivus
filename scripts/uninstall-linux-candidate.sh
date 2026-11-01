#!/usr/bin/env bash
set -euo pipefail
prefix="$HOME/.local"; if (( $# )); then prefix="$1"; fi
[[ "$prefix" = /* && "$prefix" != "/" ]] || { echo "Require an absolute non-root prefix" >&2; exit 1; }
bin="$prefix/bin/retivus"; marker="$prefix/share/retivus/installed-sha256"
[[ -f "$marker" && -f "$bin" ]] || { echo "No managed Retivus install found" >&2; exit 1; }
expected="$(cat "$marker")"
actual="$(sha256sum "$bin" | cut -d' ' -f1)"
[[ "$expected" == "$actual" ]] || { echo "Refusing to delete modified binary" >&2; exit 1; }
rm -- "$bin" "$marker" "$prefix/share/applications/retivus.desktop"
rmdir "$prefix/share/retivus" 2>/dev/null || true
echo "Uninstalled Retivus from $prefix"
