#!/usr/bin/env bash
# Pair independently produced, checksummed candidate binary/source archives.
# Neither a matching SHA nor this check certifies third-party notice compliance.
set -euo pipefail
[[ $# -eq 3 ]] || { echo 'Usage: check-source-parity.sh BINARY_ARTIFACT_DIR SOURCE_ARTIFACT_DIR COMMIT_SHA' >&2; exit 2; }
binaries="$1"
sources="$2"
expected_sha="$3"
[[ "$expected_sha" =~ ^[0-9a-f]{40}$ ]] || { echo 'Invalid commit SHA' >&2; exit 1; }
[[ -d "$binaries" && -d "$sources" ]] || { echo 'Missing artifact directories' >&2; exit 1; }
binary="$(find "$binaries" -type f -name 'retivus-*-linux-x86_64.tar.zst' -print -quit)"
source_archive="$(find "$sources" -type f -name 'retivus-source-*.tar.zst' -print -quit)"
[[ -n "$binary" && -f "$binary" ]] || { echo 'Missing binary candidate archive' >&2; exit 1; }
[[ -n "$source_archive" && -f "$source_archive" ]] || { echo 'Missing source candidate archive' >&2; exit 1; }
[[ "$(basename "$source_archive")" == "retivus-source-$expected_sha.tar.zst" ]] || { echo 'Source archive commit mismatch' >&2; exit 1; }
(cd "$(dirname "$binary")" && sha256sum --check "$(basename "$binary").sha256")
(cd "$(dirname "$source_archive")" && sha256sum --check "$(basename "$source_archive").sha256")
grep -Fx "Root commit: $expected_sha" "$sources/SOURCE-AUDIT.txt"
grep -Fx 'Offline Cargo metadata: PASS' "$sources/SOURCE-AUDIT.txt"
test -s "$sources/THIRD-PARTY-SOURCES.tsv"
test -s "$sources/LICENSE-FILE-AUDIT.tsv"
test -s "$sources/PARENT-NOTICE-REVIEW.tsv"
test -s "$sources/LINUX-TARGET-DEPENDENCIES.tsv"
test -s "$sources/LINUX-TARGET-NOTICE-REVIEW.tsv"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
tar --zstd -xf "$binary" -C "$tmp"
binary_root="$(find "$tmp" -mindepth 1 -maxdepth 1 -type d -name 'retivus-*-linux-x86_64' -print -quit)"
[[ -n "$binary_root" ]] || { echo 'Missing candidate package root' >&2; exit 1; }
grep -Fx "Retivus source commit: $expected_sha" "$binary_root/share/doc/retivus/SOURCE-PROVENANCE.txt"
binary_lock="$binary_root/share/doc/retivus/source/Cargo.lock"
test -s "$binary_lock"
cmp "$binary_lock" <(tar --zstd -xOf "$source_archive" source/Cargo.lock)
lock_sha="$(sha256sum "$binary_lock" | cut -d' ' -f1)"
grep -Fx "Retivus Cargo.lock sha256: $lock_sha" "$binary_root/share/doc/retivus/SOURCE-PROVENANCE.txt"
grep -Fx "Cargo.lock sha256: $lock_sha" "$sources/SOURCE-AUDIT.txt"
cmp "$sources/THIRD-PARTY-SOURCES.tsv" <(tar --zstd -xOf "$source_archive" source/THIRD-PARTY-SOURCES.tsv)
cmp "$sources/LICENSE-FILE-AUDIT.tsv" <(tar --zstd -xOf "$source_archive" source/LICENSE-FILE-AUDIT.tsv)
cmp "$sources/PARENT-NOTICE-REVIEW.tsv" <(tar --zstd -xOf "$source_archive" source/PARENT-NOTICE-REVIEW.tsv)
cmp "$sources/LINUX-TARGET-DEPENDENCIES.tsv" <(tar --zstd -xOf "$source_archive" source/LINUX-TARGET-DEPENDENCIES.tsv)
cmp "$sources/LINUX-TARGET-NOTICE-REVIEW.tsv" <(tar --zstd -xOf "$source_archive" source/LINUX-TARGET-NOTICE-REVIEW.tsv)
cmp "$binary_root/share/doc/retivus/source/docs/release-notice-exceptions.md" <(tar --zstd -xOf "$source_archive" source/docs/release-notice-exceptions.md)
cmp "$binary_root/share/doc/retivus/source/LICENSES/upstream/htmlescape-0.3.1/DEBIAN-PACKAGING-ATTRIBUTION.md" <(tar --zstd -xOf "$source_archive" "source/LICENSES/upstream/htmlescape-0.3.1/DEBIAN-PACKAGING-ATTRIBUTION.md")
for notice in LICENSE-MIT LICENSE-APACHE; do
  cmp "$binary_root/share/doc/retivus/source/LICENSES/upstream/intl_pluralrules-7.0.2/$notice" <(tar --zstd -xOf "$source_archive" "source/LICENSES/upstream/intl_pluralrules-7.0.2/$notice")
done
cmp "$binary_root/share/doc/retivus/source/LICENSES/upstream/EXTRA-NOTICE-EVIDENCE.tsv" <(tar --zstd -xOf "$source_archive" "source/LICENSES/upstream/EXTRA-NOTICE-EVIDENCE.tsv")
# The source candidate must preserve the pinned parent notices, not merely
# their source URLs. The source-bundle audit checks these against upstream.
for notice in LICENSE-MIT LICENSE-APACHE; do
  test -s "$binary_root/share/doc/retivus/source/LICENSES/upstream/egui-0.33.3/$notice" || {
    # The installer embeds a lightweight source tree; full upstream notices
    # are provided by the independently checksummed corresponding source.
    tar --zstd -xOf "$source_archive" "source/LICENSES/upstream/egui-0.33.3/$notice" >/dev/null
  }
done
index="$tmp/source-index.txt"
tar --zstd -tf "$source_archive" > "$index"
for required in source/.cargo/config.toml source/vendor/anki-0.0.0/Cargo.toml source/vendor/percent-encoding-iri-2.2.0/Cargo.toml; do
    grep -Fx "$required" "$index"
done
echo 'PASS: binary and vendored-source archives have matching commit, lockfile and notice inventories.'
