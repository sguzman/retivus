#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
test -x target/release/retivus || { echo "Build target/release/retivus first" >&2; exit 1; }
test -f Cargo.lock || { echo "A resolved Cargo.lock is required" >&2; exit 1; }
version="$(sed -n '/^\[package\]/,/^\[/{s/^version = "\([^"]*\)"/\1/p;}' Cargo.toml | head -n 1)"
[[ -n "$version" ]] || { echo "Missing Cargo package version" >&2; exit 1; }
name="retivus-$version-linux-x86_64"
out="dist"; if (( $# )); then out="$1"; fi
mkdir -p "$out"; out="$(realpath "$out")"
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
root="$work/$name"
mkdir -p "$root/bin" "$root/share/applications" "$root/share/doc/retivus/source"
install -m755 target/release/retivus "$root/bin/retivus"
for item in LICENSE LICENSES/ANKI-ATTRIBUTION.md LICENSES/MIT-original.txt README.md; do
  install -m644 "$item" "$root/share/doc/retivus/$(basename "$item")"
done
printf '%s\n' "$version" > "$root/VERSION"
cat > "$root/share/applications/retivus.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Retivus
Comment=Source-independent spaced repetition
Exec=retivus
Terminal=false
Categories=Education;
DESKTOP
git archive --format=tar HEAD | tar -x -C "$root/share/doc/retivus/source"
install -m644 Cargo.lock "$root/share/doc/retivus/source/Cargo.lock"
install -m755 scripts/install-linux-candidate.sh "$root/install.sh"
install -m755 scripts/uninstall-linux-candidate.sh "$root/uninstall.sh"
cat > "$root/RELEASE-CANDIDATE.txt" <<'NOTICE'
UNRELEASED TEST CANDIDATE — NOT AN ACCEPTED OR PUBLISHED RETIVUS RELEASE.
Includes Retivus source and resolved Cargo.lock. Linked Anki rslib source is
pinned by Cargo.toml; see upstream attribution. Corresponding-source/license
review, actual Desktop interoperability and release compatibility remain open.
No Anki user profiles or private study records are packaged.
NOTICE
stamp="$(git log -1 --format=%ct)"
tar --sort=name --mtime="@$stamp" --owner=0 --group=0 --numeric-owner -C "$work" -cf - "$name" | zstd -T1 -19 -o "$out/$name.tar.zst"
(cd "$out" && sha256sum "$name.tar.zst" > "$name.tar.zst.sha256")
echo "Candidate: $out/$name.tar.zst"
