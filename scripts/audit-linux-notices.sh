#!/usr/bin/env bash
# Classify the all-platform parent-notice audit against the normal/build
# dependency graph of the actual Linux release target. This does not certify
# which source bytes a compiler linked, nor clear any license obligation.
set -euo pipefail
out="${1:?Usage: audit-linux-notices.sh AUDIT_DIRECTORY}"
target=x86_64-unknown-linux-gnu
test -s "$out/PARENT-NOTICE-REVIEW.tsv"
test -d "$out/source/vendor"
test -s "$out/source/.cargo/config.toml"
deps="$out/LINUX-TARGET-DEPENDENCIES.tsv"
report="$out/LINUX-TARGET-NOTICE-REVIEW.tsv"
cargo_home="$(mktemp -d)"
trap 'rm -rf "$cargo_home"' EXIT
# Cargo's own target-aware tree excludes dev edges and non-Linux target
# dependencies, while retaining the host build dependencies that can affect
# a reproducible Linux binary. The source is already a fully vendored,
# checksummed candidate; no external Cargo cache is permitted.
(
  cd "$out/source"
  CARGO_HOME="$cargo_home" CARGO_NET_OFFLINE=true \
    cargo tree --locked --offline --features anki-backend \
    --target "$target" --edges normal,build --prefix none --format '{p}'
) | awk '$2 ~ /^v[0-9]/ {print $1 "\t" substr($2,2)}' | LC_ALL=C sort -u > "$deps"
test -s "$deps"
grep -Fxq $'retivus\t0.1.0' "$deps"
grep -Fxq $'anki\t0.0.0' "$deps"
awk -F '\t' 'BEGIN {OFS=FS}
  NR==FNR { reachable[$1 FS $2]=1; next }
  {print $0, (($1 FS $2) in reachable ? "LINUX_NORMAL_OR_BUILD" : "NOT_IN_LINUX_NORMAL_OR_BUILD")}
' "$deps" "$out/PARENT-NOTICE-REVIEW.tsv" > "$report"
test -s "$report"
test "$(wc -l < "$report")" -eq "$(wc -l < "$out/PARENT-NOTICE-REVIEW.tsv")"
awk -F '\t' '
  NF!=9 {print "Malformed target notice row:", $0 > "/dev/stderr"; exit 1}
  $5=="REVIEW_PARENT_OR_UPSTREAM" {
    if ($9=="LINUX_NORMAL_OR_BUILD") active++;
    else if ($9=="NOT_IN_LINUX_NORMAL_OR_BUILD") other++;
    else {print "Bad target classification" > "/dev/stderr"; exit 1}
  }
  END {printf "Linux normal/build unresolved notice entries: %d; not in Linux graph: %d.\n", active, other}
' "$report"
echo 'Target reachability is only a scoped review input; no notices have been legally cleared.'
