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
# These are byte-for-byte upstream notices from egui release 0.33.3.
# Git blob verification prevents a locally edited or version-mismatched file
# from being treated as reviewed upstream evidence.
egui_notices="$out/source/LICENSES/upstream/egui-0.33.3"
test -s "$egui_notices/LICENSE-MIT"
test -s "$egui_notices/LICENSE-APACHE"
test "$(git hash-object "$egui_notices/LICENSE-MIT")" = "673ea5f0505d6fa65c6a2b6a753c93b8e19d11b9"
test "$(git hash-object "$egui_notices/LICENSE-APACHE")" = "11069edd79019f7dafbe3138841cf289209270dd"
# The pinned Anki source checkout was verified upstream before this step.
anki_notices="$out/source/LICENSES"
test -s "$anki_notices/ANKI-PINNED-LICENSE.txt"
test -s "$anki_notices/ANKI-PINNED-CONTRIBUTORS.txt"
count=0
evidenced=0
pending=0
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
  review=REVIEW_PARENT_OR_UPSTREAM
  evidence=-
  if [[ "$upstream" == "https://github.com/ankitects/anki" && "$version" == 0.0.0 ]]; then
    review=PINNED_PARENT_NOTICES_PRESENT
    evidence=LICENSES/ANKI-PINNED-LICENSE.txt,LICENSES/ANKI-PINNED-CONTRIBUTORS.txt
  elif [[ "$upstream" == https://github.com/emilk/egui* && "$version" == 0.33.3 ]]; then
    review=PINNED_PARENT_NOTICES_PRESENT
    evidence=LICENSES/upstream/egui-0.33.3/LICENSE-MIT,LICENSES/upstream/egui-0.33.3/LICENSE-APACHE
    if [[ "$name" == epaint_default_fonts ]]; then
      # Embedded font licenses and copyrights are distinct from egui itself.
      # They already travel with the vendored source; verify every known file.
      for font_notice in Hack-Regular.txt OFL.txt UFL.txt emoji-icon-font-mit-license.txt; do
        test -s "$out/source/vendor/$name-$version/fonts/$font_notice" || {
          echo "Missing embedded font notice $font_notice" >&2; exit 1;
        }
      done
      evidence="$evidence,source/vendor/$name-$version/fonts/{Hack-Regular.txt,OFL.txt,UFL.txt,emoji-icon-font-mit-license.txt}"
    fi
  fi
  # Evidence presence is NOT a legal clearance decision; retain every row
  # for the final M7.1 human/provenance review, including inherited notices.
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$version" "$license" "$upstream" "$review" "$evidence" >> "$report"
  if [[ "$review" == REVIEW_PARENT_OR_UPSTREAM ]]; then ((pending += 1)); else ((evidenced += 1)); fi
  ((count += 1))
done < "$out/LICENSE-FILE-AUDIT.tsv"
test "$count" -gt 0
printf 'PARENT NOTICE REVIEW: %s missing crate-local notice files, %s upstream locations declared.\n' \
  "$count" "$(grep -vc $'\tUNDECLARED\t' "$report" || true)"
printf 'Notice provenance: %s rows have pinned parent/nested evidence; %s still need source acquisition.\n' "$evidenced" "$pending"
echo 'Evidence present is not legal clearance; all entries remain within M7.1 review.'
