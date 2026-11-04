#!/usr/bin/env bash
# Synthetic evidence audit contract only: NO physical Orca session is performed.
set -euo pipefail
auditor="$(cd "$(dirname "$0")" && pwd)/audit-physical-wayland-evidence.sh"
root="$(mktemp -d)"
trap 'rm -rf -- "$root"' EXIT
evidence="$root/synthetic-test-only"
mkdir "$evidence"
printf 'MANUALLY REPORTED PASS: pending independent evidence review\n' > "$evidence/STATUS.txt"
{
  printf 'UTC start: 2026-11-04T17:55:00Z\n'
  printf 'Retivus binary SHA-256: %064d\n' 0
  printf 'Orca audible in physical desktop: manually confirmed\n'
  printf 'Session type: wayland\n'
  printf 'UTC finish: 2026-11-04T18:00:00Z\n'
} > "$evidence/session.txt"
{
  printf 'scenario\tcheck\tresult\n'
  for scene in review conflict; do
    for dimensions in 720x650 1080x800; do
      printf '%s\t%s\tp\n' "$scene-$dimensions" 'Populated content visible; no clipped or inaccessible controls'
      printf '%s\t%s\tp\n' "$scene-$dimensions" 'High contrast, reduced motion and keyboard focus work'
      if [[ "$scene" == review ]]; then
        printf '%s\t%s\tp\n' "$scene-$dimensions" 'Orca actually speaks labels and changed due/review status'
      else
        printf '%s\t%s\tp\n' "$scene-$dimensions" 'Orca actually speaks conflict/validation status changes'
      fi
    done
  done
} > "$evidence/results.tsv"
bash "$auditor" "$evidence" >/dev/null
cp "$evidence/results.tsv" "$root/expected.tsv"
sed -i '$d' "$evidence/results.tsv"
if bash "$auditor" "$evidence" >/dev/null 2>&1; then
  echo 'Auditor accepted incomplete scenarios' >&2; exit 1
fi
cp "$root/expected.tsv" "$evidence/results.tsv"
tail -n 1 "$root/expected.tsv" >> "$evidence/results.tsv"
if bash "$auditor" "$evidence" >/dev/null 2>&1; then
  echo 'Auditor accepted duplicate rows' >&2; exit 1
fi
cp "$root/expected.tsv" "$evidence/results.tsv"
sed -i 's/\tp$/\ts/' "$evidence/results.tsv"
if bash "$auditor" "$evidence" >/dev/null 2>&1; then
  echo 'Auditor accepted skipped checks' >&2; exit 1
fi
cp "$root/expected.tsv" "$evidence/results.tsv"
sed -i '2s/\t/\t\t/' "$evidence/results.tsv"
if bash "$auditor" "$evidence" >/dev/null 2>&1; then
  echo 'Auditor accepted an empty TSV column' >&2; exit 1
fi
cp "$root/expected.tsv" "$evidence/results.tsv"
sed -i '2s/$/\t/' "$evidence/results.tsv"
if bash "$auditor" "$evidence" >/dev/null 2>&1; then
  echo 'Auditor accepted an extra TSV column' >&2; exit 1
fi
cp "$root/expected.tsv" "$evidence/results.tsv"
sed -i '2s/\t/ /' "$evidence/results.tsv"
if bash "$auditor" "$evidence" >/dev/null 2>&1; then
  echo 'Auditor accepted a missing TSV field separator' >&2; exit 1
fi
cp "$root/expected.tsv" "$evidence/results.tsv"
printf 'INCOMPLETE\n' > "$evidence/STATUS.txt"
if bash "$auditor" "$evidence" >/dev/null 2>&1; then
  echo 'Auditor accepted interrupted status' >&2; exit 1
fi
echo 'PASS: Synthetic audit rejects incomplete, duplicate, skipped and interrupted records; no physical QA performed.'
