#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
prefix="$HOME/.local"; if (( $# )); then prefix="$1"; fi
[[ "$prefix" = /* && "$prefix" != "/" ]] || { echo "Require an absolute non-root prefix" >&2; exit 1; }
bin="$prefix/bin/retivus"; marker="$prefix/share/retivus/installed-sha256"
if [[ -e "$bin" && ! -f "$marker" ]]; then
  echo "Refusing to overwrite unmanaged binary $bin" >&2; exit 1
fi
mkdir -p "$prefix/bin" "$prefix/share/applications" "$prefix/share/retivus"
tmp="$bin.$$"; trap 'rm -f "$tmp"' EXIT
install -m755 "$here/bin/retivus" "$tmp"
mv -f "$tmp" "$bin"
install -m644 "$here/share/applications/retivus.desktop" "$prefix/share/applications/retivus.desktop"
sha256sum "$bin" | cut -d' ' -f1 > "$marker"
echo "Installed Retivus to $prefix"
