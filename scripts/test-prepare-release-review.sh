#!/usr/bin/env bash
# Synthetic contract QA only: no real release, credentials or learner profiles.
set -euo pipefail
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
script="$(cd "$(dirname "$0")" && pwd)/prepare-release-review.sh"
commit=0123456789abcdef0123456789abcdef01234567
binary_dir="$root/binary evidence"
source_dir="$root/source evidence"
mkdir -p "$binary_dir/dist" "$source_dir/nested"
binary="$binary_dir/dist/retivus-0.1.0-linux-x86_64.tar.zst"
source_archive="$source_dir/nested/retivus-source-$commit.tar.zst"
printf 'synthetic binary\n' > "$binary"
printf 'synthetic corresponding source\n' > "$source_archive"
printf 'synthetic lock\n' > "$binary_dir/Cargo.lock"
for archive in "$binary" "$source_archive"; do
  (cd "$(dirname "$archive")" && sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256")
done
for name in SOURCE-AUDIT.txt THIRD-PARTY-SOURCES.tsv LICENSE-FILE-AUDIT.tsv \
  PARENT-NOTICE-REVIEW.tsv LINUX-TARGET-DEPENDENCIES.tsv LINUX-TARGET-NOTICE-REVIEW.tsv; do
  printf 'synthetic evidence for %s\n' "$name" > "$source_dir/$name"
done
output="$root/reviewer packet"
bash "$script" "$binary_dir" "$source_dir" "$commit" "$output"
grep -Fx 'NOT REVIEWED - DO NOT RELEASE' "$output/STATUS.txt"
grep -F "Exact source commit: $commit" "$output/REVIEW.md"
grep -F 'Project owner publication authorization: NOT GRANTED' "$output/REVIEW.md"
(cd "$output" && sha256sum --check PACKET-SHA256SUMS > /dev/null)
# Cannot overwrite earlier reviewer evidence.
if bash "$script" "$binary_dir" "$source_dir" "$commit" "$output" >/dev/null 2>&1; then
  echo 'Unexpectedly overwrote packet' >&2
  exit 1
fi
# Reject ambiguous binary archives, without creating an output.
cp "$binary" "$binary_dir/retivus-0.2.0-linux-x86_64.tar.zst"
if bash "$script" "$binary_dir" "$source_dir" "$commit" "$root/ambiguous" >/dev/null 2>&1; then
  echo 'Accepted ambiguous archive' >&2
  exit 1
fi
test ! -e "$root/ambiguous"
rm "$binary_dir/retivus-0.2.0-linux-x86_64.tar.zst"
# Reject a sidecar that checksums an unrelated file instead of the archive.
for archive in "$binary" "$source_archive"; do
  evidence_dir="$(dirname "$archive")"
  printf 'unrelated evidence\n' > "$evidence_dir/unrelated.txt"
  (cd "$evidence_dir" && sha256sum unrelated.txt > "$(basename "$archive").sha256")
  output_bad="$root/misbound-$(basename "$archive")"
  if bash "$script" "$binary_dir" "$source_dir" "$commit" "$output_bad" >/dev/null 2>&1; then
    echo "Accepted a sidecar for an unrelated file: $archive" >&2
    exit 1
  fi
  test ! -e "$output_bad"
  (cd "$evidence_dir" && sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256")
done
# Reject a sidecar with an extra otherwise-valid checksum line.
printf '%s\n' 'extra entry' > "$binary_dir/dist/extra.txt"
(cd "$binary_dir/dist" && sha256sum extra.txt >> "$(basename "$binary").sha256")
if bash "$script" "$binary_dir" "$source_dir" "$commit" "$root/extra-sidecar" >/dev/null 2>&1; then
  echo 'Accepted an extraneous checksum entry' >&2
  exit 1
fi
test ! -e "$root/extra-sidecar"
(cd "$binary_dir/dist" && sha256sum "$(basename "$binary")" > "$(basename "$binary").sha256")
# Reject a tampered archive even when its sidecar is still present.
printf 'tampering\n' >> "$binary"
if bash "$script" "$binary_dir" "$source_dir" "$commit" "$root/tampered" >/dev/null 2>&1; then
  echo 'Accepted mismatched SHA-256' >&2
  exit 1
fi
test ! -e "$root/tampered"
echo 'PASS: nested input, unsigned packet, overwrite, ambiguity and checksum safety.'
