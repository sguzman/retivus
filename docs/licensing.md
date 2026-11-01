# Anki backend licensing decision gate

Retivus begins with original MIT-licensed Rust and egui code. Its initial source validator and GUI do **not** link against Anki. The standalone [integration spike](../spikes/anki-collection/README.md) explicitly marks its own license **AGPL-3.0-or-later** and is not distributed with the main product.

## What adopting upstream Anki's `rslib` means

Anki's upstream source and Rust backend carry the **GNU Affero GPL, version 3 or later**. If Retivus incorporates and distributes a combined work using Anki's AGPL backend, the **combined application must satisfy the AGPL**. That typically entails:

- Declaring an AGPL-compatible license for the combined program, keeping copyright/attribution information and giving recipients the license terms.
- Making corresponding source available to recipients, including build and installation scripts and source required to modify and rebuild the covered combination.
- Honoring AGPL section 13 if a modified version supports remote network interaction: affected network users must have an opportunity to obtain the corresponding source.
- Preserving the MIT notices for originally MIT-licensed components rather than pretending those authors' existing license or copyright vanished.

The **source code license is not the vocabulary data license**. User-owned decks, personal progress, review histories and independent external dataset licenses do not automatically become AGPL merely because the application is. Do not confuse AGPL source availability with posting private user files or credentials.

## Decision boundary

**Status: pending M1.4 after successful standalone API feasibility verification.**

Preferred option: when direct upstream integration is technically and legally viable, distribute Retivus's combined Anki-backed application under **AGPL-3.0-or-later**, with upstream notices and source obligations satisfied. This is not merely changing the root `LICENSE` file; actual source provenance, Cargo package metadata, SPDX headers, README documentation, build instructions, bundled notices and release assets must agree.

If we cannot responsibly satisfy those obligations or the API is unsuitable, document the precise issue and evaluate interoperable alternatives before choosing a second architecture. Do not claim a standalone process is automatically a licensing escape hatch; combined-work analysis depends on actual architecture and circumstances.

References: [Anki LICENSE](https://github.com/ankitects/anki/blob/main/LICENSE), [GNU AGPLv3 text](https://www.gnu.org/licenses/agpl-3.0.en.html), [GNU GPL FAQ](https://www.gnu.org/licenses/gpl-faq.en.html).

This is engineering compliance guidance, not a substitute for formal legal advice.
