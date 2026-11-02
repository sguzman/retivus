#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
test -x target/release/retivus || { echo "Build target/release/retivus first" >&2; exit 1; }
test -f Cargo.lock || { echo "A resolved Cargo.lock is required" >&2; exit 1; }
# The included source snapshot is exactly HEAD; never package a locally
# modified tracked tree as if its binary and source came from that commit.
[[ -z "$(git status --porcelain --untracked-files=no)" ]] || {
  echo "Refusing candidate packaging from a modified tracked source tree" >&2
  exit 1
}
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
pinned_anki="$(grep -Eo 'rev = "[0-9a-f]{40}"' Cargo.toml | head -n 1 | cut -d'"' -f2)"
test "${#pinned_anki}" -eq 40 || { echo "Missing pinned Anki revision" >&2; exit 1; }
{
  printf 'Retivus source commit: %s\n' "$(git rev-parse HEAD)"
  printf 'Retivus Cargo.lock sha256: %s\n' "$(sha256sum Cargo.lock | cut -d' ' -f1)"
  printf 'Upstream Anki rslib revision: %s\n' "$pinned_anki"
  printf 'Upstream Anki source URL: https://github.com/ankitects/anki\n'
  printf 'Upstream Anki source is pinned but NOT bundled in this candidate.\n'
  printf 'Full corresponding-source and third-party-notice review remains a release gate.\n'
} > "$root/share/doc/retivus/SOURCE-PROVENANCE.txt"
install -m755 scripts/install-linux-candidate.sh "$root/install.sh"
install -m755 scripts/uninstall-linux-candidate.sh "$root/uninstall.sh"
cat > "$root/RELEASE-CANDIDATE.txt" <<'NOTICE'
UNRELEASED TEST CANDIDATE — NOT AN ACCEPTED OR PUBLISHED RETIVUS RELEASE.
Includes Retivus source and resolved Cargo.lock. Linked Anki rslib source is
pinned by Cargo.toml; see upstream attribution. Corresponding-source/license
review and release compatibility remain open.
Official Anki Desktop GUI package interoperability passed separately in CI;
AnkiWeb sync remains unverified.
No Anki user profiles or private study records are packaged.
NOTICE
stamp="$(git log -1 --format=%ct)"
tar --sort=name --mtime="@$stamp" --owner=0 --group=0 --numeric-owner -C "$work" -cf - "$name" | zstd -T1 -19 -o "$out/$name.tar.zst"
(cd "$out" && sha256sum "$name.tar.zst" > "$name.tar.zst.sha256")
echo "Candidate: $out/$name.tar.zst"
