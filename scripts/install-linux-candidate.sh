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
# Snapshot only installer-owned paths after integrity/preflight checks.
# If any later write fails, restore them before returning failure; never touch
# the profile/database tree or any unmanaged content under the install prefix.
managed=(
  bin/retivus
  share/applications/retivus.desktop
  share/doc/retivus/LICENSE
  share/doc/retivus/ANKI-ATTRIBUTION.md
  share/doc/retivus/MIT-original.txt
  share/doc/retivus/README.md
  share/retivus/installed-sha256
  share/retivus/installed-files.sha256
)
backup="$stage/previous-install"
mutating=0
cleanup() {
  code=$?
  trap - EXIT INT TERM
  set +e
  if (( code != 0 && mutating )); then
    failed=0
    for rel in "${managed[@]}"; do
      path="$prefix/$rel"
      if [[ -f "$backup/$rel" ]]; then
        if ! cmp -s "$backup/$rel" "$path"; then
          # Refuse to follow a symlink inserted during an interrupted install.
          if [[ -L "$path" ]]; then
            echo "Unsafe symlink during rollback: $rel" >&2
            failed=1
          else
            cp -p -- "$backup/$rel" "$path" || failed=1
          fi
        fi
      elif [[ -e "$path" || -L "$path" ]]; then
        rm -f -- "$path" || failed=1
      fi
    done
    if (( failed )); then
      echo "Retivus update failed; automatic rollback was incomplete. Preserve the installation for manual recovery." >&2
    else
      echo "Retivus update failed; previous managed files restored." >&2
    fi
  fi
  [[ -z "$tmp" ]] || rm -f -- "$tmp"
  rm -rf -- "$stage"
  exit "$code"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
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
# Back up all currently owned bytes before the first change; this also
# records absence so a failed fresh install removes only its own new files.
for rel in "${managed[@]}"; do
  if [[ -e "$prefix/$rel" || -L "$prefix/$rel" ]]; then
    [[ -f "$prefix/$rel" && ! -L "$prefix/$rel" ]] || {
      echo "Invalid previously managed path before update: $rel" >&2
      exit 1
    }
    mkdir -p "$backup/$(dirname "$rel")"
    cp -p -- "$prefix/$rel" "$backup/$rel"
  fi
done
mkdir -p "$prefix/bin" "$prefix/share/applications" "$prefix/share/doc/retivus" "$prefix/share/retivus"
mutating=1
tmp="$(mktemp "$prefix/bin/.retivus.XXXXXXXXXX")"
install -m755 "$stage/bin/retivus" "$tmp"
mv -f "$tmp" "$prefix/bin/retivus"
tmp=""
# Deterministic post-replacement failure injection for the isolated contract
# test. The EXIT trap must restore the previous binary, manifest and notices.
if [[ "${RETIVUS_TEST_FAIL_AFTER_BINARY:-}" == 1 ]]; then
  echo "Injected candidate update failure after binary replacement" >&2
  exit 75
fi
install -m644 "$stage/share/applications/retivus.desktop" "$prefix/share/applications/retivus.desktop"
for item in LICENSE ANKI-ATTRIBUTION.md MIT-original.txt README.md; do
  install -m644 "$stage/share/doc/retivus/$item" "$prefix/share/doc/retivus/$item"
done
(cd "$prefix" && sha256sum -- bin/retivus share/applications/retivus.desktop share/doc/retivus/LICENSE share/doc/retivus/ANKI-ATTRIBUTION.md share/doc/retivus/MIT-original.txt share/doc/retivus/README.md) > "$manifest"
sha256sum "$prefix/bin/retivus" | cut -d' ' -f1 > "$marker"
echo "Installed Retivus binary and license notices to $prefix"
