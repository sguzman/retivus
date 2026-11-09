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
cp "$binary" "$binary_dir/another-retivus-0.1.0-linux-x86_64.tar.zst"
if bash "$script" "$binary_dir" "$source_dir" "$commit" "$root/ambiguous" >/dev/null 2>&1; then
  echo 'Accepted ambiguous archive' >&2
  exit 1
fi
test ! -e "$root/ambiguous"
rm "$binary_dir/another-retivus-0.1.0-linux-x86_64.tar.zst"
# Reject a tampered archive even when its sidecar is still present.
printf 'tampering\n' >> "$binary"
if bash "$script" "$binary_dir" "$source_dir" "$commit" "$root/tampered" >/dev/null 2>&1; then
  echo 'Accepted mismatched SHA-256' >&2
  exit 1
fi
test ! -e "$root/tampered"
echo 'PASS: nested input, unsigned packet, overwrite, ambiguity and checksum safety.'
