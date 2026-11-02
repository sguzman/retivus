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
