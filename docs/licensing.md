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

## Requirements when Anki is integrated

Anki's [upstream source](https://github.com/ankitects/anki) uses **AGPL-3.0-or-later**, with some separately licensed components. Before distributing a linked or otherwise combined Anki-backed Retivus application, confirm:

- Include required license terms and preserve upstream copyright notices, author information and relevant third-party notices.
- Supply recipients of the covered program with the corresponding source code, including required build and installation scripts, under the applicable AGPL terms.
- Comply with AGPL section 13 if a modified AGPL program offers remote network interaction.
- Include actual upstream source/commit provenance and relevant modification notices, not just an AGPL string in `Cargo.toml`.
- Verify the real distribution and dependency structure before claiming compliance. Do not assume a separate executable or IPC arrangement categorically changes combined-work analysis.

**Status:** The main application license was proactively changed; Anki `rslib` is **not currently linked into the shipping Retivus application**. The M1.4 roadmap task also requires a working, legally reviewed adapter, notices and packaging; this license commit alone does **not** complete M1.4.

## Separate data and user content

An application's AGPL license does **not** automatically require disclosure of a user's private decks, card histories, personal notes, collection databases or credentials. Data sourced from Mexilex and future providers has its own provenance and licensing. Do not treat the MIT/AGPL code licensing decision as a permission to redistribute dictionary data or private user records.

## References

- [Current Retivus LICENSE](../LICENSE)
- [Historical MIT notice](../LICENSES/MIT-original.txt)
- [Anki source license](https://github.com/ankitects/anki/blob/main/LICENSE)
- [GNU AGPLv3](https://www.gnu.org/licenses/agpl-3.0.html)
- [GNU GPL FAQ](https://www.gnu.org/licenses/gpl-faq.html)

This document describes project practice, not formal legal advice.
