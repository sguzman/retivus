# Retivus licensing, history and upstream compliance

## Current license

**GNU Affero General Public License, version 3 or later (AGPL-3.0-or-later).** The repository's root [LICENSE](../LICENSE) contains the full AGPLv3 text, and the Rust package manifest identifies the `AGPL-3.0-or-later` SPDX license expression.

### Why the license changed

> Retivus was originally intended to use the permissive MIT License. To integrate Anki's AGPL-licensed Rust backend while honoring the upstream license requirements, Retivus adopted AGPL-3.0-or-later.

This choice was made because **directly incorporating Anki's Rust backend into the distributed application** entails AGPL obligations for the resulting combined program, not because original code was inherently required to use AGPL. Retivus made that compliance decision *before* linking the Anki backend into its main application. The independent `spikes/anki-collection/` experiment already uses AGPL but is not a distributed product.

This is a deliberate architectural and compliance choice, not an allegation against Anki or a claim that the MIT license is defective. Anki's licensing is published openly and was considered when planning backend reuse.

## What remains MIT-licensed

Retivus's **original revisions** were published under MIT, with copyright attributed to Salvador Guzman in 2026. The [verbatim original MIT notice](../LICENSES/MIT-original.txt) is retained for historical provenance and compliance. **Changing the license for later versions does not revoke MIT rights that recipients already received for earlier versions.**

The repository's current license is AGPL-3.0-or-later, **not a promise that all future versions will also be available under MIT**. Original independently authored Retivus code remains owned by its copyright holders; copyright ownership and permissions are separate from the license terms for distribution.

The committed [Anki attribution notice](../LICENSES/ANKI-ATTRIBUTION.md) identifies the exact pinned backend source and the currently unmodified Cargo integration. The optional adapter's [feature-enabled CI test](https://github.com/sguzman/retivus/actions/runs/41266965280) passed. This verifies the pinned source/build boundary, **not** final release-asset source packaging or legal approval of a distributable; those remain M7 criteria.

## Requirements when Anki is integrated

Anki's [upstream source](https://github.com/ankitects/anki) uses **AGPL-3.0-or-later**, with some separately licensed components. Before distributing a linked or otherwise combined Anki-backed Retivus application, confirm:

- Include required license terms and preserve upstream copyright notices, author information and relevant third-party notices.
- Supply recipients of the covered program with the corresponding source code, including required build and installation scripts, under the applicable AGPL terms.
- Comply with AGPL section 13 if a modified AGPL program offers remote network interaction.
- Include actual upstream source/commit provenance and relevant modification notices, not just an AGPL string in `Cargo.toml`.
- Verify the real distribution and dependency structure before claiming compliance. Do not assume a separate executable or IPC arrangement categorically changes combined-work analysis.

**Status:** The main application license was proactively changed; Anki `rslib` is available behind an **opt-in `anki-backend` Cargo feature** in the application library, pinned to an exact upstream Git commit. The default build does not enable this feature; distribution of feature-enabled binaries must preserve corresponding-source access and upstream notices. There is still no public Anki-backed release. The M1.4 roadmap task also requires a working, legally reviewed adapter, notices and packaging; this license commit alone does **not** complete M1.4.

## Separate data and user content

An application's AGPL license does **not** automatically require disclosure of a user's private decks, card histories, personal notes, collection databases or credentials. Data sourced from Mexilex and future providers has its own provenance and licensing. Do not treat the MIT/AGPL code licensing decision as a permission to redistribute dictionary data or private user records.

## References

- [Current Retivus LICENSE](../LICENSE)
- [Historical MIT notice](../LICENSES/MIT-original.txt)
- [Anki source license](https://github.com/ankitects/anki/blob/main/LICENSE)
- [GNU AGPLv3](https://www.gnu.org/licenses/agpl-3.0.html)
- [GNU GPL FAQ](https://www.gnu.org/licenses/gpl-faq.html)

This document describes project practice, not formal legal advice.

## Offline corresponding-source inventory (release gate in progress)

The independent [locked Cargo source audit](../.github/workflows/source-bundle-audit.yml) vendors all resolved registry and Git dependency crates into a disposable Retivus source snapshot, records their license-expression inventory, and verifies that Cargo can resolve the feature-enabled graph with a fresh empty Cargo home and `--offline --locked`. The audit CI now retains a **checksummed, commit-specific source archive** containing the full vendored dependency tree plus a third-party SPDX-expression inventory and provenance summary. This is a separate *unpublished source candidate*, not yet a production release asset. Passing this audit is preparation for bundling registry sources and checking third-party notices in the actual release archive; it does not itself satisfy M7.1 or establish legal compliance.

## Locked dependency license-file evidence

The offline source audit generates `LICENSE-FILE-AUDIT.tsv` alongside its existing `THIRD-PARTY-SOURCES.tsv` report. For each vendored Git or registry crate it records the declared license expression and whether an identifiable license/notice file exists at the crate's top level. Entries marked `REVIEW_PARENT_OR_UPSTREAM` require manual investigation of workspace parent repositories, nested files or required attribution; a missing crate-local file **does not by itself establish a violation**. The report is also embedded in the checksummed source archive and compared byte-for-byte by the release artifact-parity job. This improves evidence for M7.1 without certifying compliance or authorizing a production release.

### Exact pinned Anki license and contributors

The corresponding-source candidate now includes `LICENSES/ANKI-PINNED-LICENSE.txt` and `LICENSES/ANKI-PINNED-CONTRIBUTORS.txt`, copied byte-for-byte from `LICENSE` and `CONTRIBUTORS` at the **same pinned Anki commit** used by the built Rust backend. The source audit refuses a mismatched or modified checkout and verifies both files. Anki's upstream `LICENSE` also identifies portions with different licensing, so including the root Retivus AGPL text alone is not sufficient notice provenance. These files provide actual upstream licensing and credit text, but **do not clear the rest of the third-party notice review queue or certify a production release**.

### Pinned Anki protobuf build inputs

A fresh `--offline` compilation exposed a missing source input which `cargo vendor` cannot infer from Cargo dependency metadata: the pinned `anki_proto` build script reads the upstream repository's sibling `proto/anki/*.proto` files. The source audit now checks out Anki commit `136bd42bfbd356fc756b6d3e28cc77b02da11ea7`, rejects a modified/different revision, and embeds its tracked `proto` directory in the source candidate. The subsequent fresh offline build check must pass before this is accepted as a complete compilation source; other upstream workspace assets may still be required. No license-compliance certification is implied.

### Pinned Anki Fluent translations and workspace version input

The independent offline source compilation also identified required `anki_i18n` build inputs outside Cargo crates: `ftl/core`, `ftl/qt`, and the upstream `ftl/core-repo` and `ftl/qt-repo` translation Git submodules. The audit now checks out only these two submodules at their exact parent Gitlink revisions, confirms clean tracked files, and archives their source content together with the required upstream `.version` file at the path expected by vendored Anki. An independently completed offline compilation—not this copying step—is the remaining acceptance test. No external Anki profile or translation service is contacted during the subsequent offline build.

## Pinned egui parent notices and embedded fonts

For the pinned egui **0.33.3** dependency family, the corresponding-source candidate now retains verbatim `LICENSE-MIT` and `LICENSE-APACHE` copied from upstream tag [`0.33.3`](https://github.com/emilk/egui/tree/0.33.3), with their Git blob hashes checked at source-audit time. This covers the shared parent-notice provenance for egui, eframe, epaint and the other same-version egui crates whose registry packages omit workspace-root files. `epaint_default_fonts` additionally requires independent font notices; the audit checks the vendored `Hack-Regular.txt`, `OFL.txt`, `UFL.txt`, and `emoji-icon-font-mit-license.txt` rather than treating the egui MIT/Apache text as covering fonts.

The generated `PARENT-NOTICE-REVIEW.tsv` now records both `PINNED_PARENT_NOTICES_PRESENT` (with exact evidence paths) and `REVIEW_PARENT_OR_UPSTREAM` (with `-` evidence). **Neither status is a legal clearance decision**: the full locked graph and any binary-distribution notice requirements remain subject to the final M7.1 compliance review.

### Pinned objc2 family parent notices

The 25 objc2 workspace-related crate entries span two upstream license-note versions. Source candidates now include the **exact Git-blob-verified upstream `LICENSE.txt` and `LICENSE.md` texts**, with per-crate tag selection for `block2-0.5.1`, `objc-sys-0.3.5`, `objc2-0.5.3`, `frameworks-0.2.2`, `dispatch2-0.3.1`, `objc2-0.6.5`, `objc2-encode-4.1.0`, and `frameworks-0.3.2`. The same exact root license blob occurs at four inspected tags in each group. The `LICENSE.md` text explicitly discusses Apple SDK-derived bindings; **this adds attribution evidence, not a legal conclusion about Apple SDKs, source distribution, or macOS binaries**. Linux binary dependency relevance must still be determined separately from locked source-graph membership.

### Pinned AccessKit notices

The eight locked AccessKit-family packages were traced to their exact upstream release tags. Two workspace revisions cover the selected crate releases. The upstream root `LICENSE-MIT`, `LICENSE-APACHE`, `AUTHORS`, and `LICENSE.chromium` have identical Git blob hashes at both revisions; copies are retained in `LICENSES/upstream/accesskit/` and hash-verified during packaging. The audit records which exact crate release tag each notice applies to, but retains the final human legal/packaging review requirement.

### Pinned rust-unic notices

All five locked `unic-*` crates are version `0.9.0` from `open-i18n/rust-unic`. The source candidate now preserves that release tag's exact root `LICENSE-MIT`, `LICENSE-APACHE`, `AUTHORS` and `COPYRIGHT.md` with independent Git blob hash checks. Review status remains evidence-present, not legal clearance.

### Further pinned upstream crate notices

`LICENSES/upstream/EXTRA-NOTICE-EVIDENCE.tsv` records 11 additional dependency entries and their published Cargo VCS commits, source-path notices, and original upstream Git blob hashes (AT-SPI, clipboard-win, Fluent, JNI, NDK, profiling, prost-reflect and valuable). The audit fails on missing, modified, overlapping, stale or version-mismatched evidence. Shared texts are deduplicated only after identical Git blob verification. **Presence is not a legal clearance decision**, and binary notice/distribution obligations remain to be reviewed.

## Upstream parent-notice review queue

The source audit also generates **`PARENT-NOTICE-REVIEW.tsv`** for dependencies without a recognized crate-local notice. It records each crate name/version, declared SPDX expression, and its Cargo-manifest upstream repository URL (or the known pinned Anki upstream repository for Anki's workspace crates). The report is retained beside and embedded in the checksummed source archive; binary/source parity must compare the two copies exactly. This transforms the unresolved 72 entries into a traceable upstream review queue. **A repository URL is not notice text and does not certify legal compatibility or resolve any entry; each actual corresponding copyright/license notice still requires review.**
