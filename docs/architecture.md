# Architecture and integration decisions

## ADR-001: First evaluate direct use of Anki's Rust backend

**Status:** proposed, evidence-backed reconnaissance; feasibility remains unverified.

The upstream [ankitects/anki](https://github.com/ankitects/anki) repository has a large `rslib` Rust crate, including public modules for `collection`, `card`, `notes`, `notetype`, `decks`, `scheduler`, `import_export`, `sync`, and `media`. The crate is named `anki`, has `publish = false` and `workspace = ".."` in its manifest, and uses extensive other workspace crates, generated protobuf code, assets and build dependencies. A direct one-line crates.io dependency is therefore **not** an established integration strategy.

### Feasibility decision gate (M1)

1. Pin an upstream commit and build Anki's actual Rust workspace in a dedicated CI spike, recording toolchain and Linux dependencies. Do not vendor entire upstream sources without proven benefit.
2. Compile a **minimal external Rust client** that opens an *isolated, disposable* Anki collection using `anki::collection::CollectionBuilder`, creates a deck, note/card, and retrieves a review queue via documented actual APIs. Verify tests against this pinned commit. No real user collection or credentials in tests.
3. Inventory required crates, generated artifacts, native deps, upgrade friction, relevant upstream AGPL obligations and runtime-size impact.
4. If practical, adopt a pinned backend adapter and formally review project licensing first. If blocked, record exact failures and evaluate Anki's protobuf backend service or a narrow clean-room compatible adapter. A fallback must not imply full Anki ecosystem compatibility.
5. Keep egui concerns out of the engine adapter; the application depends on stable internal traits. Maintain Anki-compatible identifiers and review actions only after verification of upstream semantics.

**Licensing gate:** Retivus was initially MIT-licensed, but has adopted **AGPL-3.0-or-later** in anticipation of direct use of Anki's AGPL-licensed `rslib`. The original MIT notice is retained for historical revisions. This proactive project-license change is **not sufficient proof of integration compliance**: pinned upstream provenance, actual dependency boundaries, required notices, corresponding-source and packaging obligations still must be verified before a combined release. See [licensing history](licensing.md).

## ADR-002: Source-provider boundary

A provider supplies source edition, snapshot identity, language pair, stable entry/sense identifiers, word, meanings, provenance and optional metadata. The importer validates content; *application-owned* IDs, cards, decks and review history remain separate. The initial v1 JSONL validator is deliberately Anki-backend-independent.

**Import transaction:** verify file checksums and manifest first, stream records through validated staging storage, generate a dry-run diff, and commit with rollback only after all records pass. Duplicates and ambiguous meaning mappings are blocked. Provider URL and snapshot are not substitutes for immutable hashes.

**Migration:** new snapshot is compared against the old; exact identities can retain links; changed annotation and candidate IDs require explicit review; missing source records are archived while their cards/history persist. No blind merge.

## ADR-003: Presentation

Rust with eframe/egui, keyboard-first, System/Light/Dark theme, persistent preferences. Respect font licenses; prefer user-installed Krypton Nerd Font if discovered in a safe platform-specific lookup, otherwise native defaults. Avoid a handcrafted rendering engine before baseline egui works.

The [brand system](branding.md) supplies a single OKLCH hue with contrast-adjusted light and dark accents, applied via egui's separate theme styles without defeating System mode. Color tokens are code-owned, not hardcoded per screen.

## ADR-004: Storage ownership

A Retivus profile is the boundary for its own cards, note/deck metadata, preferences, source snapshots, credentials and event log. The adapter must avoid mixing provider-owned metadata with private review state. Use upstream collection storage if the backend spike succeeds; do not create a competing overlapping SRS database without reason.

## Technical risks to resolve

- Anki backend's unpublished workspace and generated protobuf requirements.
- AGPL obligations and upstream version pinning.
- Matching Anki's note/card/template and media semantics for exported packages.
- Atomic migration of user-facing collections and restored backups.
- AnkiWeb protocol/credentials; never assume network compatibility from local `.apkg` success.
- Provider volume: Mexilex has 124,444 default study entries and must not be loaded into one unbounded GUI frame.

## ADR-005: Opt-in pinned Anki backend module

**Status: CI-verified optional integration, not a complete user-facing backend.** Direct Cargo Git consumption of upstream `anki` at commit `136bd42bfbd356fc756b6d3e28cc77b02da11ea7` was verified by the independent [Git dependency integration run](https://github.com/sguzman/retivus/actions/runs/41266535589). Retivus now offers an optional `anki-backend` feature with a small wrapper in `src/anki_backend.rs`. This keeps default Rust/egui builds fast and avoids requiring protobuf code generation until integration is explicitly enabled. The feature adds Tokio `io-util` for upstream feature unification without changing Anki source files.

The initial wrapper only opens an **in-memory Anki collection**, creates a real deck and Basic note, and queries due queues. It deliberately does **not** open, modify, or migrate any existing Anki profile. Persistent storage, note templates and review grading remain later milestones. [Feature-enabled Anki CI 41266965280](https://github.com/sguzman/retivus/actions/runs/41266965280) passed direct library integration tests; the independent default app CI also passed. The adapter is therefore verified at the source integration boundary but not yet production-complete: profile durability, editing, backups, review grading, package interchange and sync have separate roadmap tasks.

## ADR-006: Retivus-owned collection paths and backend schema gates

The initial optional Anki adapter exposes `create_managed_profile(root, id)` and `open_managed_profile(root, id)`, not any implicit scan or open of a user's preexisting Anki collection. The caller must explicitly supply a **preexisting root directory**. Profile IDs are restricted to 1–64 ASCII letters, digits, hyphens and underscores; creation uses an exclusive new directory so it cannot replace an existing profile. Reopening requires real directories and regular (non-symlink) collection files. Integrity checks are requested on reopen. Missing or incompatible databases are reported as errors, **never reset**.

Initial schema compatibility tests create only temporary Retivus-owned profile fixtures, downgrade a fixture to upstream schema 11 and verify reopening/upgrade to schema 18 preserves notes, then assert a future schema version 99 is rejected without deleting the collection. The upstream pin supports schema 11–18; later upstream revisions require their own explicit tests. These tests do not constitute live-user upgrade safety, locking safeguards against concurrent file modifications, backups or Anki collection import; those remain part of M2/M4/M6.
