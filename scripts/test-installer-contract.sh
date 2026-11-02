#!/usr/bin/env bash
# Isolated zero-network release installer contract suite; runs without Cargo.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
sandbox="$(mktemp -d)"
trap 'rm -rf "$sandbox"' EXIT
bundle="$sandbox/retivus-candidate"
mkdir -p "$bundle/bin" "$bundle/share/applications" "$bundle/share/doc/retivus"
printf '#!/bin/sh\nexit 0\n' > "$bundle/bin/retivus"
chmod 755 "$bundle/bin/retivus"
printf '[Desktop Entry]\nType=Application\nName=Retivus\nExec=retivus\n' > "$bundle/share/applications/retivus.desktop"
cp "$repo/LICENSE" "$bundle/share/doc/retivus/LICENSE"
cp "$repo/LICENSES/ANKI-ATTRIBUTION.md" "$bundle/share/doc/retivus/ANKI-ATTRIBUTION.md"
cp "$repo/LICENSES/MIT-original.txt" "$bundle/share/doc/retivus/MIT-original.txt"
cp "$repo/README.md" "$bundle/share/doc/retivus/README.md"
cp "$repo/scripts/install-linux-candidate.sh" "$bundle/install.sh"
cp "$repo/scripts/uninstall-linux-candidate.sh" "$bundle/uninstall.sh"
chmod 755 "$bundle/install.sh" "$bundle/uninstall.sh"
reject() {
  if "$@" > "$sandbox/rejected.log" 2>&1; then
    echo "Expected refusal, but command succeeded: $*" >&2
    exit 1
  fi
}
prefix="$sandbox/clean install with spaces/.local"
mkdir -p "$prefix"
printf 'PERSONAL COLLECTION - MUST SURVIVE\n' > "$prefix/learner.sqlite3"
sha="$(sha256sum "$prefix/learner.sqlite3" | cut -d' ' -f1)"
"$bundle/install.sh" "$prefix"
test -x "$prefix/bin/retivus"
for item in LICENSE ANKI-ATTRIBUTION.md MIT-original.txt README.md; do
  cmp "$bundle/share/doc/retivus/$item" "$prefix/share/doc/retivus/$item"
done
(cd "$prefix" && sha256sum --check --status share/retivus/installed-files.sha256)
"$bundle/install.sh" "$prefix"
# Existing managed binary is replaceable on a later release, and only on a
# successful original-manifest verification.
printf '#!/bin/sh\nexit 7\n' > "$bundle/bin/retivus"
chmod 755 "$bundle/bin/retivus"
"$bundle/install.sh" "$prefix"
cmp "$bundle/bin/retivus" "$prefix/bin/retivus"
# A malformed upgrade bundle must fail before replacing a verified install.
binary_before="$(sha256sum "$prefix/bin/retivus" | cut -d' ' -f1)"
manifest_before="$(sha256sum "$prefix/share/retivus/installed-files.sha256" | cut -d' ' -f1)"
mv "$bundle/share/doc/retivus/README.md" "$sandbox/README.saved"
reject "$bundle/install.sh" "$prefix"
mv "$sandbox/README.saved" "$bundle/share/doc/retivus/README.md"
test "$(sha256sum "$prefix/bin/retivus" | cut -d' ' -f1)" = "$binary_before"
test "$(sha256sum "$prefix/share/retivus/installed-files.sha256" | cut -d' ' -f1)" = "$manifest_before"
mv "$bundle/bin/retivus" "$sandbox/retivus.saved"
ln -s "$sandbox/retivus.saved" "$bundle/bin/retivus"
reject "$bundle/install.sh" "$prefix"
rm "$bundle/bin/retivus"
mv "$sandbox/retivus.saved" "$bundle/bin/retivus"
test "$(sha256sum "$prefix/bin/retivus" | cut -d' ' -f1)" = "$binary_before"
test "$(sha256sum "$prefix/share/retivus/installed-files.sha256" | cut -d' ' -f1)" = "$manifest_before"
# Neither updater nor remover may discard any modified user-visible notice.
printf 'user annotation\n' >> "$prefix/share/doc/retivus/LICENSE"
reject "$bundle/install.sh" "$prefix"
reject "$bundle/uninstall.sh" "$prefix"
test -e "$prefix/bin/retivus"
cp "$bundle/share/doc/retivus/LICENSE" "$prefix/share/doc/retivus/LICENSE"
# Binary edits also block all destructive removal.
printf 'user changes\n' >> "$prefix/bin/retivus"
reject "$bundle/uninstall.sh" "$prefix"
cp "$bundle/bin/retivus" "$prefix/bin/retivus"
"$bundle/uninstall.sh" "$prefix"
test ! -e "$prefix/bin/retivus"
test ! -e "$prefix/share/doc/retivus/ANKI-ATTRIBUTION.md"
test "$(sha256sum "$prefix/learner.sqlite3" | cut -d' ' -f1)" = "$sha"
# Explicitly preserve untracked binaries and license notices.
unmanaged="$sandbox/unmanaged"
mkdir -p "$unmanaged/bin"
printf 'different app\n' > "$unmanaged/bin/retivus"
reject "$bundle/install.sh" "$unmanaged"
grep -q 'different app' "$unmanaged/bin/retivus"
protected="$sandbox/protected notice"
mkdir -p "$protected/share/doc/retivus"
printf 'legal user note\n' > "$protected/share/doc/retivus/LICENSE"
reject "$bundle/install.sh" "$protected"
grep -q 'legal user note' "$protected/share/doc/retivus/LICENSE"
# Migration of old candidates must be explicit and preserve personal data.
old="$sandbox/old"
mkdir -p "$old/bin" "$old/share/applications" "$old/share/retivus"
cp "$bundle/bin/retivus" "$old/bin/retivus"
cp "$bundle/share/applications/retivus.desktop" "$old/share/applications/retivus.desktop"
sha256sum "$old/bin/retivus" | cut -d' ' -f1 > "$old/share/retivus/installed-sha256"
printf 'private progress\n' > "$old/learner.db"
"$bundle/install.sh" "$old"
test -s "$old/share/retivus/installed-files.sha256"
"$bundle/uninstall.sh" "$old"
grep -q 'private progress' "$old/learner.db"
# A symlinked managed directory must never redirect writes/deletions outside
# the explicitly selected install prefix, including during removal.
outside="$sandbox/real outsider"
mkdir -p "$outside"
redirected="$sandbox/symlink bin"
mkdir -p "$redirected"
ln -s "$outside" "$redirected/bin"
reject "$bundle/install.sh" "$redirected"
test ! -e "$outside/retivus"
redirected_docs="$sandbox/symlink docs"
mkdir -p "$redirected_docs/share/doc"
ln -s "$outside" "$redirected_docs/share/doc/retivus"
reject "$bundle/install.sh" "$redirected_docs"
test ! -e "$outside/LICENSE"
linked_prefix="$sandbox/linked prefix"
ln -s "$outside" "$linked_prefix"
reject "$bundle/install.sh" "$linked_prefix"
reject "$bundle/uninstall.sh" "$linked_prefix"
echo "PASS: managed install/update/legacy migration, notice integrity, rollback safety, study-data survival and symlink/unmanaged-file refusals."
