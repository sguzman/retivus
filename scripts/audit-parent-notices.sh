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
# objc2 workspace license texts are identical at these exact upstream tags:
# pre-transition: block2-0.5.1, objc-sys-0.3.5, objc2-0.5.3,
#                 frameworks-0.2.2
# newer:          dispatch2-0.3.1, objc2-0.6.5,
#                 objc2-encode-4.1.0, frameworks-0.3.2
# Each tag's upstream root file was inspected and has the Git blob hash
# checked below. These are notice evidence, not OS X SDK legal clearance.
objc_notices="$out/source/LICENSES/upstream/objc2"
test -s "$objc_notices/LICENSE.txt"
test -s "$objc_notices/LICENSE.md"
test "$(git hash-object "$objc_notices/LICENSE.txt")" = "e9ab3babc0d8602b83dccde8a462f8f3dc1e52ad"
test "$(git hash-object "$objc_notices/LICENSE.md")" = "540ae4c81adb5f64a9f61e222b688880da24300e"
# All eight locked AccessKit packages map to two verified workspace commits:
# accesskit-v0.21.1 / accesskit_consumer-v0.30.1 = 0b6901a8...
# remaining packages at their exact version tags = 5801c4a4...
# These four root blobs are identical in both upstream revisions.
accesskit_notices="$out/source/LICENSES/upstream/accesskit"
test -s "$accesskit_notices/LICENSE-MIT"
test -s "$accesskit_notices/LICENSE-APACHE"
test -s "$accesskit_notices/LICENSE.chromium"
test -s "$accesskit_notices/AUTHORS"
test "$(git hash-object "$accesskit_notices/LICENSE-MIT")" = "31aa79387f27e730e33d871925e152e35e428031"
test "$(git hash-object "$accesskit_notices/LICENSE-APACHE")" = "1b5ec8b78e237b5c3b3d812a7c0a6589d0f7161d"
test "$(git hash-object "$accesskit_notices/LICENSE.chromium")" = "a32e00ce6be3620adca152e53734f7de8fda38cf"
test "$(git hash-object "$accesskit_notices/AUTHORS")" = "432f1b7bb779a9082fd10a4ef1e4f29c44b512c3"
# The five unic-* crates at version 0.9.0 share the release-tagged
# rust-unic workspace root notices (v0.9.0, commit 89f5f55...).
unic_notices="$out/source/LICENSES/upstream/rust-unic-0.9.0"
test "$(git hash-object "$unic_notices/AUTHORS")" = "ab3575c1f490363e4957adbc77a3783c48f4492b"
test "$(git hash-object "$unic_notices/COPYRIGHT.md")" = "0322ceec0b8f15aa2d74a6173c07cc8c6f3764b3"
test "$(git hash-object "$unic_notices/LICENSE-APACHE")" = "16fe87b06e802f094b3fbb0894b137bca2b16ef1"
test "$(git hash-object "$unic_notices/LICENSE-MIT")" = "31aa79387f27e730e33d871925e152e35e428031"
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
  if [[ "$upstream" == "https://github.com/madsmtm/objc2" ]]; then
    objc_tag=
    objc_file=
    case "$name/$version" in
      block2/0.5.1) objc_tag=block2-0.5.1; objc_file=LICENSE.txt ;;
      objc-sys/0.3.5) objc_tag=objc-sys-0.3.5; objc_file=LICENSE.txt ;;
      objc2/0.5.3) objc_tag=objc2-0.5.3; objc_file=LICENSE.txt ;;
      objc2-*/0.2.2) objc_tag=frameworks-0.2.2; objc_file=LICENSE.txt ;;
      dispatch2/0.3.1) objc_tag=dispatch2-0.3.1; objc_file=LICENSE.md ;;
      objc2/0.6.5) objc_tag=objc2-0.6.5; objc_file=LICENSE.md ;;
      objc2-encode/4.1.0) objc_tag=objc2-encode-4.1.0; objc_file=LICENSE.md ;;
      objc2-*/0.3.2) objc_tag=frameworks-0.3.2; objc_file=LICENSE.md ;;
    esac
    if [[ -n "$objc_tag" ]]; then
      review=PINNED_PARENT_NOTICES_PRESENT
      evidence="LICENSES/upstream/objc2/$objc_file@${objc_tag}"
    fi
  fi
  if [[ "$upstream" == "https://github.com/AccessKit/accesskit" ]]; then
    ak_tag=
    case "$name/$version" in
      accesskit/0.21.1) ak_tag=accesskit-v0.21.1 ;;
      accesskit_atspi_common/0.14.2) ak_tag=accesskit_atspi_common-v0.14.2 ;;
      accesskit_consumer/0.30.1) ak_tag=accesskit_consumer-v0.30.1 ;;
      accesskit_consumer/0.31.0) ak_tag=accesskit_consumer-v0.31.0 ;;
      accesskit_macos/0.22.2) ak_tag=accesskit_macos-v0.22.2 ;;
      accesskit_unix/0.17.2) ak_tag=accesskit_unix-v0.17.2 ;;
      accesskit_windows/0.29.2) ak_tag=accesskit_windows-v0.29.2 ;;
      accesskit_winit/0.29.2) ak_tag=accesskit_winit-v0.29.2 ;;
    esac
    if [[ -n "$ak_tag" ]]; then
      review=PINNED_PARENT_NOTICES_PRESENT
      evidence="LICENSES/upstream/accesskit/{LICENSE-MIT,LICENSE-APACHE,AUTHORS,LICENSE.chromium}@${ak_tag}"
    fi
  fi
  if [[ "$upstream" == "https://github.com/open-i18n/rust-unic/" && "$version" == "0.9.0" && "$name" == unic-* ]]; then
    review=PINNED_PARENT_NOTICES_PRESENT
    evidence="LICENSES/upstream/rust-unic-0.9.0/{LICENSE-MIT,LICENSE-APACHE,AUTHORS,COPYRIGHT.md}@v0.9.0"
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
