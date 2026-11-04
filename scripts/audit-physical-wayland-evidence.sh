#!/usr/bin/env bash
# Check only the STRUCTURE of private physical Wayland/Orca QA evidence.
# Does not run Retivus/Orca or certify any human actually heard speech.
set -euo pipefail
if [[ $# -ne 1 ]]; then
  echo 'Usage: bash scripts/audit-physical-wayland-evidence.sh /private/evidence-dir' >&2
  exit 2
fi
dir="$1"
if [[ ! -d "$dir" || -L "$dir" ]]; then
  echo 'Evidence must be an existing real directory' >&2
  exit 2
fi
for filename in results.tsv session.txt STATUS.txt; do
  if [[ ! -f "$dir/$filename" || -L "$dir/$filename" ]]; then
    echo "Missing or unsafe evidence file: $filename" >&2
    exit 1
  fi
done
if [[ "$(cat "$dir/STATUS.txt")" != 'MANUALLY REPORTED PASS: pending independent evidence review' ]]; then
  echo 'Evidence is not marked manually complete and pending review' >&2
  exit 1
fi
if ! grep -Eq '^Retivus binary SHA-256: [0-9a-f]{64}$' "$dir/session.txt" ||
   ! grep -Fqx 'Orca audible in physical desktop: manually confirmed' "$dir/session.txt" ||
   ! grep -Eq '^UTC start: [0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$' "$dir/session.txt" ||
   ! grep -Eq '^UTC finish: [0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$' "$dir/session.txt" ||
   ! grep -Fqx 'Session type: wayland' "$dir/session.txt"; then
  echo 'Missing binary provenance, audible-speech confirmation, Wayland, or completion time' >&2
  exit 1
fi
declare -A expected=()
for scene in review conflict; do
  for dimensions in 720x650 1080x800; do
    prefix="$scene-$dimensions|"
    expected["$prefix""Populated content visible; no clipped or inaccessible controls"]=1
    expected["$prefix""High contrast, reduced motion and keyboard focus work"]=1
    if [[ "$scene" == review ]]; then
      expected["$prefix""Orca actually speaks labels and changed due/review status"]=1
    else
      expected["$prefix""Orca actually speaks conflict/validation status changes"]=1
    fi
  done
done
{
  IFS= read -r header || { echo 'No evidence header' >&2; exit 1; }
  if [[ "$header" != $'scenario\tcheck\tresult' ]]; then
    echo 'Malformed evidence header' >&2
    exit 1
  fi
  count=0
  # IFS splitting collapses adjacent tabs and can discard empty fields.
  # Match exactly three nonempty tab-separated cells and a literal pass.
  row_pattern=$'^([^\t]+)\t([^\t]+)\t(p)$'
  while IFS= read -r line || [[ -n "$line" ]]; do
    if [[ ! "$line" =~ $row_pattern ]]; then
      echo 'Malformed or non-passing physical QA TSV row' >&2
      exit 1
    fi
    scene="${BASH_REMATCH[1]}"
    check="${BASH_REMATCH[2]}"
    key="$scene|$check"
    if [[ ! -v expected["$key"] ]]; then
      echo "Unexpected or duplicate physical QA row: $key" >&2
      exit 1
    fi
    unset 'expected[$key]'
    count=$((count + 1))
  done
  if (( count != 12 )); then
    echo "Incomplete physical QA evidence: $count/12 unique passing rows" >&2
    exit 1
  fi
} < "$dir/results.tsv"
echo 'PASS: Physical QA evidence structure complete (12/12 recorded rows).'
echo 'NOT ACCEPTED: Independent human review of actual physical Wayland speech is still required.'
