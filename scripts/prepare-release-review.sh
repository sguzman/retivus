#!/usr/bin/env bash
# Prepare an explicitly UNREVIEWED packet from a paired release-candidate run.
# The caller must first run scripts/check-source-parity.sh on these archives.
# This script separately rechecks both archive SHA-256 sidecars.
set -euo pipefail
if [[ $# -ne 4 ]]; then
  echo 'Usage: prepare-release-review.sh BINARY_ARTIFACT_DIR SOURCE_ARTIFACT_DIR COMMIT_SHA NEW_OUTPUT_DIR' >&2
  exit 2
fi
binary_dir="$1"
source_dir="$2"
commit="$3"
output="$4"
if [[ ! "$commit" =~ ^[a-f0-9]{40}$ ]]; then
  echo 'Commit must be exactly 40 lowercase hex characters' >&2
  exit 2
fi
if [[ ! -d "$binary_dir" || ! -d "$source_dir" || -L "$binary_dir" || -L "$source_dir" ]]; then
  echo 'Both inputs must be real artifact directories' >&2
  exit 2
fi
if [[ -e "$output" || -L "$output" ]]; then
  echo 'Output already exists; refusing to overwrite review evidence' >&2
  exit 2
fi
# Downloaded artifacts may contain dist/ or other nested paths. Fail closed
# on missing/ambiguous candidates instead of hashing an empty path.
binary_count="$(find "$binary_dir" -type f -name 'retivus-*-linux-x86_64.tar.zst' | wc -l)"
source_count="$(find "$source_dir" -type f -name "retivus-source-$commit.tar.zst" | wc -l)"
if [[ "$binary_count" -ne 1 || "$source_count" -ne 1 ]]; then
  echo "Expected exactly one binary and one same-commit source archive (found $binary_count and $source_count)" >&2
  exit 1
fi
binary="$(find "$binary_dir" -type f -name 'retivus-*-linux-x86_64.tar.zst' -print -quit)"
source_archive="$(find "$source_dir" -type f -name "retivus-source-$commit.tar.zst" -print -quit)"
[[ -s "$binary" && -s "$source_archive" && -s "$binary_dir/Cargo.lock" ]] || {
  echo 'Missing release candidate, corresponding source, or Cargo.lock' >&2
  exit 1
}
for archive in "$binary" "$source_archive"; do
  sidecar="$archive.sha256"
  [[ -s "$sidecar" && ! -L "$sidecar" ]] || {
    echo "Missing non-symlinked checksum sidecar: $sidecar" >&2
    exit 1
  }
  (cd "$(dirname "$archive")" && sha256sum --check "$(basename "$sidecar")") >/dev/null
done
for name in SOURCE-AUDIT.txt THIRD-PARTY-SOURCES.tsv LICENSE-FILE-AUDIT.tsv \
  PARENT-NOTICE-REVIEW.tsv LINUX-TARGET-DEPENDENCIES.tsv LINUX-TARGET-NOTICE-REVIEW.tsv; do
  [[ -s "$source_dir/$name" && ! -L "$source_dir/$name" ]] || {
    echo "Missing or unsafe source audit inventory: $name" >&2
    exit 1
  }
done
binary_hash="$(sha256sum "$binary" | cut -d' ' -f1)"
source_hash="$(sha256sum "$source_archive" | cut -d' ' -f1)"
lock_hash="$(sha256sum "$binary_dir/Cargo.lock" | cut -d' ' -f1)"
run_id="not recorded (local packet)"
if [[ -n "${GITHUB_RUN_ID:-}" ]]; then
  run_id="$GITHUB_RUN_ID"
fi
umask 077
mkdir -m 700 "$output"
printf 'NOT REVIEWED - DO NOT RELEASE\n' > "$output/STATUS.txt"
for name in SOURCE-AUDIT.txt THIRD-PARTY-SOURCES.tsv LICENSE-FILE-AUDIT.tsv \
  PARENT-NOTICE-REVIEW.tsv LINUX-TARGET-DEPENDENCIES.tsv LINUX-TARGET-NOTICE-REVIEW.tsv; do
  cp -- "$source_dir/$name" "$output/$name"
done
cat > "$output/REVIEW.md" <<EOF
# Retivus independent release review — NOT APPROVED

- Exact source commit: $commit
- Producing GitHub Actions run: $run_id
- Binary archive: $(basename "$binary")
- Binary SHA-256: $binary_hash
- Corresponding-source archive: $(basename "$source_archive")
- Corresponding-source SHA-256: $source_hash
- Cargo.lock SHA-256: $lock_hash

The archive sidecars were checked again by this script. In the Linux
candidate workflow, the **preceding** step independently checks the complete
binary/source parity, embedded commit, lockfile, and notice inventories. This
packet does not independently prove binary/source content parity when invoked
outside that workflow. Neither mode establishes legal sufficiency or release
approval. Do not publish a version, tag, or release based on this packet.

## Still requiring independent decision

- Reviewer name and affiliation: UNASSIGNED
- UTC review date: UNRECORDED
- Independently reverified archive hashes: NO
- AGPL/Anki/MIT and corresponding-source sufficiency: PENDING
- Linux-target and all-platform notice applicability: PENDING
- dispatch 0.2.0 author-companion MIT notice decision: PENDING
- htmlescape 0.3.1 Debian attribution: PENDING
- hexf-parse 0.2.1 Debian CC0 evidence: PENDING
- Other upstream/workspace notice and data-provider rights: PENDING
- Physical Wayland/Orca M6.4 observation: NONE
- M7 installation, upgrade and recovery review: PENDING
- Reviewer disposition and accepted exceptions: NO RELEASE
- Project owner publication authorization: NOT GRANTED
EOF
(
  cd "$output"
  sha256sum SOURCE-AUDIT.txt THIRD-PARTY-SOURCES.tsv LICENSE-FILE-AUDIT.tsv \
    PARENT-NOTICE-REVIEW.tsv LINUX-TARGET-DEPENDENCIES.tsv \
    LINUX-TARGET-NOTICE-REVIEW.tsv REVIEW.md STATUS.txt > PACKET-SHA256SUMS
)
grep -Fx 'NOT REVIEWED - DO NOT RELEASE' "$output/STATUS.txt"
echo "PASS: unsigned release-review packet prepared for $commit"
