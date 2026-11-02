#!/usr/bin/env bash
# Verify the exact locked source set is sufficient for offline Cargo resolution.
# Source copies remain internal to this audit; release packaging is a separate gate.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
if (( $# )); then out="$1"; else out="$repo/dist/source-audit"; fi
mkdir -p "$out"
out="$(realpath "$out")"
test -f "$repo/Cargo.lock"
command -v cargo >/dev/null
command -v jq >/dev/null
command -v git >/dev/null
command -v tar >/dev/null
test ! -e "$out/vendor" || {
  echo "Refusing to overwrite previous vendor output: $out/vendor" >&2
  exit 1
}
# cargo vendor prints Cargo's exact source replacement configuration.
(cd "$out" && cargo vendor --manifest-path "$repo/Cargo.toml" \
  --locked --versioned-dirs vendor > vendor-config.toml)
grep -Fq 'directory = "vendor"' "$out/vendor-config.toml" || {
  echo "Expected portable relative vendor directory in Cargo configuration" >&2
  exit 1
}
mkdir -p "$out/source/.cargo"
git -C "$repo" archive --format=tar HEAD | tar -xf - -C "$out/source"
install -m644 "$repo/Cargo.lock" "$out/source/Cargo.lock"
cp "$out/vendor-config.toml" "$out/source/.cargo/config.toml"
mv "$out/vendor" "$out/source/vendor"
# Capture each locked package's declared SPDX expression or missing-license
# state for later notice review. This is an inventory, not legal certification.
cargo metadata --manifest-path "$repo/Cargo.toml" --locked \
  --features anki-backend --format-version 1 |
  jq -r '.packages[] | [.name, .version, (.license // "UNDECLARED"), (.source // "local")] | @tsv' |
  LC_ALL=C sort > "$out/THIRD-PARTY-SOURCES.tsv"
test -s "$out/THIRD-PARTY-SOURCES.tsv"
# Record concrete license-file evidence, including missing top-level files.
bash "$repo/scripts/audit-license-files.sh" "$out"
install -m644 "$out/THIRD-PARTY-SOURCES.tsv" "$out/source/THIRD-PARTY-SOURCES.tsv"
install -m644 "$out/LICENSE-FILE-AUDIT.tsv" "$out/source/LICENSE-FILE-AUDIT.tsv"
registry_count="$(grep -c '^source = "registry+' "$repo/Cargo.lock")"
vendored_count="$(find "$out/source/vendor" -mindepth 2 -maxdepth 2 -name '.cargo-checksum.json' -type f | wc -l)"
if (( vendored_count < registry_count )); then
  echo "Incomplete source cache: vendor $vendored_count vs registry $registry_count" >&2
  exit 1
fi
# Use a new Cargo home to prove no runner cache or external Git checkout.
empty_home="$(mktemp -d)"
trap 'rm -rf "$empty_home"' EXIT
# Cargo searches .cargo/config.toml relative to the invocation directory,
# not a remote --manifest-path. Run inside the copied source tree so its
# vendored Git and registry replacements are actually loaded.
(
  cd "$out/source"
  CARGO_HOME="$empty_home" CARGO_NET_OFFLINE=true \
    cargo metadata --locked --offline --features anki-backend \
    --format-version 1 >/dev/null
)
{
  printf 'Root commit: %s\n' "$(git -C "$repo" rev-parse HEAD)"
  printf 'Cargo.lock sha256: %s\n' "$(sha256sum "$repo/Cargo.lock" | cut -d' ' -f1)"
  printf 'Registry packages: %s\n' "$registry_count"
  printf 'Vendored crates: %s\n' "$vendored_count"
  printf 'Offline Cargo metadata: PASS\n'
  printf 'Source/release licensing and notices: NOT YET CERTIFIED\n'
} > "$out/SOURCE-AUDIT.txt"
cat "$out/SOURCE-AUDIT.txt"
