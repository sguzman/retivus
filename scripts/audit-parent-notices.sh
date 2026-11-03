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
# These are the actual crate-local license texts in the upstream workspace at
# the exact published source VCS commit. crates.io omitted them in 7.0.2.
plural_notices="$out/source/LICENSES/upstream/intl_pluralrules-7.0.2"
test "$(git hash-object "$plural_notices/LICENSE-MIT")" = "31aa79387f27e730e33d871925e152e35e428031"
test "$(git hash-object "$plural_notices/LICENSE-APACHE")" = "1b5ec8b78e237b5c3b3d812a7c0a6589d0f7161d"
# The pinned Anki source checkout was verified upstream before this step.
anki_notices="$out/source/LICENSES"
test -s "$anki_notices/ANKI-PINNED-LICENSE.txt"
test -s "$anki_notices/ANKI-PINNED-CONTRIBUTORS.txt"
# Immutable source-commit-specific mappings for additional upstream notices.
extra_manifest="$out/source/LICENSES/upstream/EXTRA-NOTICE-EVIDENCE.tsv"
test -s "$extra_manifest"
declare -A extra_sha extra_evidence
extra_count=0
while IFS=$'\t' read -r crate version published_sha specs; do
  [[ "$published_sha" =~ ^[0-9a-f]{40}$ ]] || { echo "Bad VCS SHA for $crate" >&2; exit 1; }
  key="$crate/$version"
  [[ -z "${extra_sha[$key]+x}" ]] || { echo "Duplicate extra notice row: $key" >&2; exit 1; }
  [[ -n "$specs" ]] || { echo "Missing notice paths: $key" >&2; exit 1; }
  IFS=',' read -r -a items <<< "$specs"
  for item in "${items[@]}"; do
    file="${item%@*}"; sha="${item##*@}"
    [[ "$file" == LICENSES/upstream/* && "$sha" =~ ^[0-9a-f]{40}$ ]] || { echo "Bad notice mapping: $item" >&2; exit 1; }
    test -s "$out/source/$file" || { echo "Missing notice: $file" >&2; exit 1; }
    test "$(git hash-object "$out/source/$file")" = "$sha" || { echo "Notice altered: $file" >&2; exit 1; }
  done
  extra_sha["$key"]="$published_sha"
  extra_evidence["$key"]="$specs"
  ((extra_count += 1))
done < "$extra_manifest"
count=0
evidenced=0
pending=0
extra_matched=0
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
  vcs_file="$out/source/vendor/$name-$version/.cargo_vcs_info.json"
  vcs_sha=UNAVAILABLE
  vcs_path=-
  if [[ -s "$vcs_file" ]]; then
    vcs_sha="$(jq -er '.git.sha1 // empty' "$vcs_file")"
    vcs_path="$(jq -r '.path_in_vcs // ""' "$vcs_file")"
    [[ "$vcs_sha" =~ ^[0-9a-f]{40}$ ]] || { echo "Invalid VCS provenance for $name $version" >&2; exit 1; }
  fi
  review=REVIEW_PARENT_OR_UPSTREAM
  evidence=-
  expected_sha=
  if [[ "$upstream" == "https://github.com/ankitects/anki" && "$version" == 0.0.0 ]]; then
    review=PINNED_PARENT_NOTICES_PRESENT
    evidence=LICENSES/ANKI-PINNED-LICENSE.txt,LICENSES/ANKI-PINNED-CONTRIBUTORS.txt
    # Anki is a pinned Git dependency; registry .cargo_vcs_info is absent.
    vcs_sha=136bd42bfbd356fc756b6d3e28cc77b02da11ea7
    expected_sha="$vcs_sha"
  elif [[ "$upstream" == https://github.com/emilk/egui* && "$version" == 0.33.3 ]]; then
    review=PINNED_PARENT_NOTICES_PRESENT
    evidence=LICENSES/upstream/egui-0.33.3/LICENSE-MIT,LICENSES/upstream/egui-0.33.3/LICENSE-APACHE
    expected_sha=44cdd653e2317d300fb8a6c9c36b03f23991e803
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
    objc_sha=
    case "$name/$version" in
      block2/0.5.1) objc_tag=block2-0.5.1; objc_file=LICENSE.txt; objc_sha=4fc083f1c6d6784577e38b0ee8dbd344481e2fd2 ;;
      objc-sys/0.3.5) objc_tag=objc-sys-0.3.5; objc_file=LICENSE.txt; objc_sha=4fc083f1c6d6784577e38b0ee8dbd344481e2fd2 ;;
      objc2/0.5.3) objc_tag=objc2-0.5.3; objc_file=LICENSE.txt; objc_sha=5237cbf081d04ef75a1f6a207b4018e7c1ae7438 ;;
      objc2-*/0.2.2) objc_tag=frameworks-0.2.2; objc_file=LICENSE.txt; objc_sha=e282618be4c3a3b9542957e0c8540e9588472ce8 ;;
      dispatch2/0.3.1) objc_tag=dispatch2-0.3.1; objc_file=LICENSE.md; objc_sha=8852b424193ca41602281b3d7540d7c8ed51e49a ;;
      objc2/0.6.5) objc_tag=objc2-0.6.5; objc_file=LICENSE.md; objc_sha=d7d2fa23ceaa5e6096c923b081040e5d81b3b9df ;;
      objc2-encode/4.1.0) objc_tag=objc2-encode-4.1.0; objc_file=LICENSE.md; objc_sha=8d214f5477365ffcbcbb7de058c86ed9a518efb7 ;;
      objc2-*/0.3.2) objc_tag=frameworks-0.3.2; objc_file=LICENSE.md; objc_sha=7b1abfd750a2cacaea71d6a56ecfb83cb7de560b ;;
    esac
    if [[ -n "$objc_tag" ]]; then
      review=PINNED_PARENT_NOTICES_PRESENT
      evidence="LICENSES/upstream/objc2/$objc_file@${objc_tag}"
      expected_sha="$objc_sha"
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
      case "$name/$version" in
        accesskit/0.21.1|accesskit_consumer/0.30.1) expected_sha=0b6901a8c8e6fb1cb5a2dc7c19dc7bee1cc5ad09 ;;
        *) expected_sha=5801c4a49ff9b2c9ec3f77b3c31272d0da4c5cd3 ;;
      esac
    fi
  fi
  if [[ "$upstream" == "https://github.com/open-i18n/rust-unic/" && "$version" == "0.9.0" && "$name" == unic-* ]]; then
    review=PINNED_PARENT_NOTICES_PRESENT
    evidence="LICENSES/upstream/rust-unic-0.9.0/{LICENSE-MIT,LICENSE-APACHE,AUTHORS,COPYRIGHT.md}@v0.9.0"
    if [[ "$name" == unic-ucd-category ]]; then
      expected_sha=8a6ce83063d90b91ae2ce59eddb803edd393fca9
    else
      expected_sha=5878605364af97a3358368a6eaef02104af2e016
    fi
  fi
  # difflib 0.4.0 predates Cargo's VCS provenance metadata. Reconcile
  # its original published Cargo.toml and all four Rust source modules
  # byte-for-byte with upstream commit f035fb8 rather than inventing SHA1.
  if [[ "$name/$version" == "difflib/0.4.0" && "$vcs_sha" == UNAVAILABLE ]]; then
    for item in \
      "src/lib.rs@ca6b1cc38b974d95769240c63f47a043083c9d02" \
      "src/differ.rs@5caef9a7a8d34c7a2c2f8cdb737ded2ed7a9f1d6" \
      "src/sequencematcher.rs@4157808631acc62605fd6b446d17523ae8a86152" \
      "src/utils.rs@b711bf82ba0e14439d80806afb70722e60195e03" \
      "Cargo.toml.orig@2edf6df6a6fc159647a10691c025786ca594cdf8"; do
      file="${item%@*}"
      sha="${item##*@}"
      test "$(git hash-object "$out/source/vendor/$name-$version/$file")" = "$sha" || {
        echo "diff-lib source no longer matches pinned upstream: $file" >&2; exit 1;
      }
    done
    notice_path="$out/source/LICENSES/upstream/difflib-0.4.0/LICENSE"
    test -s "$notice_path"
    test "$(git hash-object "$notice_path")" = "5acedf0412212a99997b78bd36da90c8a91111a7"
    review=SOURCE_MATCHED_PARENT_NOTICE_PRESENT
    evidence=LICENSES/upstream/difflib-0.4.0/LICENSE@f035fb8e656f27119e23eca9d5b996df14f3885e
  fi
  if [[ "$name/$version" == "intl_pluralrules/7.0.2" ]]; then
    [[ "$upstream" == "https://github.com/zbraniecki/pluralrules" ]] || {
      echo "Intl plural rules source URL changed" >&2; exit 1;
    }
    [[ "$license" == "Apache-2.0/MIT" ]] || {
      echo "Intl plural rules declared license changed" >&2; exit 1;
    }
    review=PINNED_PARENT_NOTICES_PRESENT
    evidence="LICENSES/upstream/intl_pluralrules-7.0.2/LICENSE-MIT,LICENSES/upstream/intl_pluralrules-7.0.2/LICENSE-APACHE"
    expected_sha=d0715d5b7cca72e228d6e6c4de154bd30dfdd06a
  fi
  # htmlescape 0.3.1 predates Cargo's VCS metadata. Verify every
  # published file against immutable upstream source commit 1699b539.
  # Its declared license is not a substitute for an upstream license file:
  # leave this row UNRESOLVED even when source matching succeeds.
  if [[ "$name/$version" == "htmlescape/0.3.1" ]]; then
    [[ "$vcs_sha" == UNAVAILABLE ]] || {
      echo "Unexpected htmlescape VCS metadata; recheck provenance" >&2; exit 1;
    }
    [[ "$upstream" == "https://github.com/veddan/rust-htmlescape" ]] || {
      echo "htmlescape upstream changed; review required" >&2; exit 1;
    }
    [[ "$license" == "Apache-2.0 / MIT / MPL-2.0" ]] || {
      echo "htmlescape license declaration changed; review required" >&2; exit 1;
    }
    for item in \
      "Cargo.toml@984837d3f069934a04e4a308e8daf45a9efa4c4c" \
      "README.md@e0bb73c91d6434787694f88887fbf549dcb4b91d" \
      ".travis.yml@45e8f4f7df3064e2564373be8f7367a29d2d3c43" \
      "moonstone-short.txt@93795fe41e4ac1d63d3bbaddf733b2a56e6ec923" \
      "src/lib.rs@098a9792917c0928d2028e944bb0e8b147b89eea" \
      "src/encode.rs@242a3cf4e95a1bc22c805ce0df56e3d24aa0fe70" \
      "src/decode.rs@18478ee274d1c1dc203c9651c3ea18084cfe035a" \
      "src/entities.rs@3f635c6f2d07a968051c7bf7bc03146c3fe78a24" \
      "src/io_support.rs@18b7a25fff0d478efa7a9b54d0867afe553d6d3d" \
      "tests/test.rs@c5d33f491c0d0978080b8819375843d4515d14ea" \
      "benches/bench.rs@dcdb9f8de9eb9c5b003fdb14c5d5c0b3032730f1"; do
      file="${item%@*}"; expected="${item##*@}"
      test "$(git hash-object "$out/source/vendor/$name-$version/$file")" = "$expected" || {
        echo "htmlescape source differs from pinned upstream commit: $file" >&2; exit 1;
      }
    done
    evidence=SOURCE_MATCHED_ONLY:1699b539179798e705ad8464128492a0a0092876
  fi
  # Validate exact published crate VCS commits before mapping upstream notices.
  key="$name/$version"
  if [[ -n "${extra_sha[$key]+x}" ]]; then
    [[ "$review" == REVIEW_PARENT_OR_UPSTREAM ]] || { echo "Overlapping notice mapping: $key" >&2; exit 1; }
    review=PINNED_PARENT_NOTICES_PRESENT
    evidence="${extra_evidence[$key]}"
    expected_sha="${extra_sha[$key]}"
    ((extra_matched += 1))
  fi
  # The upstream root notice Git blobs also matched these published commits.
  if [[ -n "$expected_sha" && "$vcs_sha" != "$expected_sha" ]]; then
    echo "VCS source revision changed for reviewed crate $name $version: $vcs_sha, expected $expected_sha" >&2
    exit 1
  fi
  # Evidence presence is NOT a legal clearance decision. Retain unresolved
  # and evidenced rows, including the published crate source revision.
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$version" "$license" "$upstream" "$review" "$evidence" "$vcs_sha" "$vcs_path" >> "$report"
  if [[ "$review" == REVIEW_PARENT_OR_UPSTREAM ]]; then ((pending += 1)); else ((evidenced += 1)); fi
  ((count += 1))
done < "$out/LICENSE-FILE-AUDIT.tsv"
test "$count" -gt 0
(( extra_matched == extra_count )) || { echo "Stale notice manifest: $extra_matched/$extra_count matched" >&2; exit 1; }
printf 'PARENT NOTICE REVIEW: %s missing crate-local notice files, %s upstream locations declared.\n' \
  "$count" "$(grep -vc $'\tUNDECLARED\t' "$report" || true)"
printf 'Notice provenance: %s rows have pinned parent/nested evidence; %s still need source acquisition.\n' "$evidenced" "$pending"
echo 'Evidence present is not legal clearance; all entries remain within M7.1 review.'
