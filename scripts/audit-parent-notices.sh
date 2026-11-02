#!/usr/bin/env bash
# Reproducible *review queue*, NOT a declaration of resolved license duties.
# A missing crate-local license may inherit a parent workspace notice.
set -euo pipefail
out="$1"
test -n "$out"
test -s "$out/LICENSE-FILE-AUDIT.tsv"
test -d "$out/source/vendor"
report="$out/PARENT-NOTICE-REVIEW.tsv"
: > "$report"
count=0
while IFS=$'\t' read -r name version license status notice; do
  [[ "$status" == REVIEW_PARENT_OR_UPSTREAM ]] || continue
  manifest="$out/source/vendor/$name-$version/Cargo.toml"
  test -s "$manifest" || { echo "Missing manifest for $name $version" >&2; exit 1; }
  upstream="$(sed -nE 's/^repository = "([^"]+)"$/\1/p' "$manifest" | head -n 1)"
  if [[ -z "$upstream" ]]; then
    case "$name" in
      anki|anki_*) upstream=https://github.com/ankitects/anki ;;
      *) upstream=UNDECLARED ;;
    esac
  fi
  printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$version" "$license" "$upstream" REVIEW_PARENT_OR_UPSTREAM >> "$report"
  ((count += 1))
done < "$out/LICENSE-FILE-AUDIT.tsv"
test "$count" -gt 0
printf 'PARENT NOTICE REVIEW: %s missing crate-local notice files, %s upstream locations declared.\n' \
  "$count" "$(grep -vc $'\tUNDECLARED\t' "$report" || true)"
echo 'All entries remain unresolved until actual corresponding notices are reviewed.'
