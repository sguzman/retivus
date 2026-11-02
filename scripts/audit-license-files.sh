#!/usr/bin/env bash
# Identify license/notice file evidence without assuming SPDX declarations
# alone are sufficient or that a missing top-level notice violates a license.
set -euo pipefail
out="${1:?Usage: audit-license-files.sh AUDIT_DIRECTORY}"
test -s "$out/THIRD-PARTY-SOURCES.tsv"
test -d "$out/source/vendor"
report="$out/LICENSE-FILE-AUDIT.tsv"
: > "$report"
shopt -s nullglob
count=0
review=0
while IFS=$'\t' read -r name version expression source; do
  [[ "$source" != local ]] || continue
  dir="$out/source/vendor/$name-$version"
  [[ -d "$dir" ]] || { echo "Missing vendored crate: $name $version" >&2; exit 1; }
  evidence=()
  for candidate in "$dir"/[Ll][Ii][Cc][Ee][Nn][Ss][Ee]* \
                   "$dir"/[Ll][Ii][Ss][Ee][Nn][Cc][Ee]* \
                   "$dir"/[Cc][Oo][Pp][Yy][Ii][Nn][Gg]* \
                   "$dir"/[Nn][Oo][Tt][Ii][Cc][Ee]* \
                   "$dir"/[Cc][Oo][Pp][Yy][Rr][Ii][Gg][Hh][Tt]* \
                   "$dir"/[Aa][Uu][Tt][Hh][Oo][Rr][Ss]* \
                   "$dir"/[Uu][Nn][Ll][Ii][Cc][Ee][Nn][Ss][Ee]* \
                   "$dir"/[Mm][Pp][Ll]-* \
                   "$dir"/[Ll][Gg][Pp][Ll]-* \
                   "$dir"/[Gg][Pp][Ll]-* \
                   "$dir"/[Aa][Pp][Aa][Cc][Hh][Ee]-* \
                   "$dir"/[Mm][Ii][Tt].*; do
    [[ -f "$candidate" && ! -L "$candidate" ]] || continue
    evidence+=("${candidate##*/}")
  done
  status=TOP_LEVEL_FILE_PRESENT
  notice='-'
  if (( ${#evidence[@]} )); then
    notice="$(IFS=,; echo "${evidence[*]}")"
  else
    status=REVIEW_PARENT_OR_UPSTREAM
    (( review += 1 ))
  fi
  printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$version" "$expression" "$status" "$notice" >> "$report"
  (( count += 1 ))
done < "$out/THIRD-PARTY-SOURCES.tsv"
test "$count" -gt 0
printf 'NOTICE FILE INVENTORY: %s vendored crates; %s need parent, upstream or nested notice review.\n' "$count" "$review"
echo 'An inventory is not a license-compliance certification.'
