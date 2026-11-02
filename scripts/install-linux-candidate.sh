#!/usr/bin/env bash
# Install/update Retivus-owned files without overwriting unmanaged content.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
prefix="$HOME/.local"; if (( $# )); then prefix="$1"; fi
[[ "$prefix" = /* && "$prefix" != "/" && ! -L "$prefix" ]] || {
  echo "Require an absolute non-root, non-symlink install prefix" >&2
  exit 1
}
# Refuse symlinked prefixes, ancestor components, managed directories and
# managed files. Without this, a symlink under the prefix could redirect
# install or removal to an unrelated path outside the selected directory.
[[ "$prefix" == "$(realpath -m -- "$prefix")" ]] || {
  echo "Install prefix must not traverse a symlink or contain dot segments" >&2
  exit 1
}
for folder in "$prefix" "$prefix/bin" "$prefix/share" "$prefix/share/applications" "$prefix/share/doc" "$prefix/share/doc/retivus" "$prefix/share/retivus"; do
  [[ ! -L "$folder" ]] || { echo "Refusing symlinked managed directory: $folder" >&2; exit 1; }
done
for rel in bin/retivus share/applications/retivus.desktop share/doc/retivus/LICENSE share/doc/retivus/ANKI-ATTRIBUTION.md share/doc/retivus/MIT-original.txt share/doc/retivus/README.md share/retivus/installed-sha256 share/retivus/installed-files.sha256; do
  [[ ! -L "$prefix/$rel" ]] || { echo "Refusing symlinked managed file: $rel" >&2; exit 1; }
done
marker="$prefix/share/retivus/installed-sha256"
manifest="$prefix/share/retivus/installed-files.sha256"
if [[ -e "$manifest" || -L "$manifest" ]]; then
  [[ -f "$manifest" && ! -L "$manifest" ]] || { echo "Invalid install manifest" >&2; exit 1; }
  (cd "$prefix" && sha256sum --check --status "$manifest") || {
    echo "Installed Retivus files changed; refusing to overwrite" >&2
    exit 1
  }
elif [[ -e "$marker" || -L "$marker" ]]; then
  # Previous candidate tracked only the binary; upgrade it conservatively.
  [[ -f "$marker" && ! -L "$marker" && -f "$prefix/bin/retivus" ]] || exit 1
  [[ "$(sha256sum "$prefix/bin/retivus" | cut -d' ' -f1)" == "$(cat "$marker")" ]] || {
    echo "Existing Retivus binary has been modified" >&2; exit 1
  }
  cmp -s "$prefix/share/applications/retivus.desktop" "$here/share/applications/retivus.desktop" || {
    echo "Existing desktop entry has been modified" >&2; exit 1
  }
  for rel in share/doc/retivus/LICENSE share/doc/retivus/ANKI-ATTRIBUTION.md share/doc/retivus/MIT-original.txt share/doc/retivus/README.md; do
    [[ ! -e "$prefix/$rel" && ! -L "$prefix/$rel" ]] || {
      echo "Refusing to overwrite unmanaged notice: $rel" >&2; exit 1
    }
  done
else
  for rel in bin/retivus share/applications/retivus.desktop share/doc/retivus/LICENSE share/doc/retivus/ANKI-ATTRIBUTION.md share/doc/retivus/MIT-original.txt share/doc/retivus/README.md; do
    [[ ! -e "$prefix/$rel" && ! -L "$prefix/$rel" ]] || {
      echo "Refusing to overwrite unmanaged file: $rel" >&2; exit 1
    }
  done
fi
# Stage every required candidate file before modifying any installed file.
# Copy the bytes, never follow source symlinks; a malformed bundle must fail
# before replacing even the executable of an otherwise healthy installation.
stage="$(mktemp -d)"
tmp=""
cleanup() {
  [[ -z "$tmp" ]] || rm -f -- "$tmp"
  rm -rf -- "$stage"
}
trap cleanup EXIT
for rel in bin/retivus share/applications/retivus.desktop share/doc/retivus/LICENSE share/doc/retivus/ANKI-ATTRIBUTION.md share/doc/retivus/MIT-original.txt share/doc/retivus/README.md; do
  source="$here/$rel"
  [[ -f "$source" && ! -L "$source" && -r "$source" ]] || {
    echo "Candidate file missing, unreadable or symlinked: $rel" >&2
    exit 1
  }
  mkdir -p "$stage/$(dirname "$rel")"
  mode=644
  [[ "$rel" != bin/retivus ]] || mode=755
  install -m "$mode" -- "$source" "$stage/$rel"
done
mkdir -p "$prefix/bin" "$prefix/share/applications" "$prefix/share/doc/retivus" "$prefix/share/retivus"
tmp="$prefix/bin/retivus.$"
install -m755 "$stage/bin/retivus" "$tmp"
mv -f "$tmp" "$prefix/bin/retivus"
tmp=""
install -m644 "$stage/share/applications/retivus.desktop" "$prefix/share/applications/retivus.desktop"
for item in LICENSE ANKI-ATTRIBUTION.md MIT-original.txt README.md; do
  install -m644 "$stage/share/doc/retivus/$item" "$prefix/share/doc/retivus/$item"
done
(cd "$prefix" && sha256sum -- bin/retivus share/applications/retivus.desktop share/doc/retivus/LICENSE share/doc/retivus/ANKI-ATTRIBUTION.md share/doc/retivus/MIT-original.txt share/doc/retivus/README.md) > "$manifest"
sha256sum "$prefix/bin/retivus" | cut -d' ' -f1 > "$marker"
echo "Installed Retivus binary and license notices to $prefix"
