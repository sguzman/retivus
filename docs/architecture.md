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

## ADR-007: Backend-owned deck, note and template operations

The feature-gated adapter offers deck listing/creation, note-type listing, a two-direction note-type creator using Anki card templates, note creation/edit/deletion, in-session undo and profile reopening through upstream Anki transactions. A disposable integration fixture tests two generated cards for one note, changing its two fields, deleting/undoing it and restoring its content after closing/reopening the collection. Retivus does **not** yet display these operations in the native GUI. No homegrown scheduling or collection schema is added. A user-facing editor, guard prompts and later review/persistence features remain separate tasks.


**M2.1 backend acceptance (2026-11-01):** [feature test run 41268857882](https://github.com/sguzman/retivus/actions/runs/41268857882) passed 20 tests with the new bidirectional note/card generation, edit/delete/undo and reopen fixture. The engine's remaining review, backup, and UI integration work is tracked independently.

## ADR-008: Explicit provider sense selection and upstream FSRS grading

**Status:** implemented behind the feature gate; M2.2 and M3.3 verified in Anki feature CI.

A provider selection is the tuple (provider ID, snapshot, entry ID, sense ID), not a word string or a language pair alone. `SourceCatalog::select_sense` verifies every component before materializing a `SourceCardCandidate`. Anki note generation is a distinct explicit action; its provenance tag is reversible and contains no private review history. Changing a provider snapshot must not silently rewrite an existing learner card.

The selected Anki backend owns scheduling. `AnkiBackend::enable_fsrs` configures upstream FSRS, and `grade_next_card` obtains the pinned upstream queue state, applies the chosen Again/Hard/Good/Easy transition through Anki's own `answer_card`, and records upstream revlog events. The feature-gated test exercises closing/reopening before a review and after an answered review. M2.2 has passed the pinned upstream four-rating transition and undo/restart fixtures; this is **not** a claim of full future scheduler parity or completed GUI study controls (M5). CLI/source catalog operations never open personal Anki profiles automatically.

## ADR-009: Non-destructive managed-profile backup and recovery

**Status:** verified collection-backup and safe restore in feature CI 41312321565; release media and power-failure tests remain separately scheduled.

Retivus captures a consistent SQLite database snapshot through SQLite `VACUUM INTO`, avoiding byte-copying an active WAL database. Snapshots are integrity-checked, flushed, and published with no-clobber semantics; no existing backup is silently replaced. Backup is refused for in-memory or unmanaged collections. Restore validates its input, copies it to an isolated staging directory under the selected root, checks it with the pinned Anki collection builder (including registered `unicase` collation), and publishes a complete SQLite database to a **new** managed profile, refusing existing profile names. On same-root filesystems publication uses an atomic hard-link operation and directory sync; a power loss before completion can leave an *empty* newly created directory but cannot expose a partially written database. Cleanup of a failed, still-empty directory is best effort. These are collection-data backups, **not** promises of media-folder capture or `.apkg` interoperability; those are separate M4 tasks. No operation reads or replaces the user's ordinary Anki profile.

## ADR-010: Explicit source snapshot reconciliation without destroying review history

**Status:** verified in Rust CI 41313849974 and Anki feature CI 41313808023.

Snapshot differences are joined by full (provider, snapshot, entry ID, sense ID) identities and returned in bounded batches. Same-spelling homographs with changed identities surface as missing+added; there is no silent spelling-based resolution. A user-approved one-to-one mapping is stored transactionally in the independent source SQLite catalog and can be undone without ever touching private study history. When an explicit mapping is applied to an existing Anki note, Retivus first checks immutable provenance, exactly one source tag and unchanged source-managed fields; source conflicts and manual edits are rejected. The pinned upstream `update_note` transaction preserves the existing card/review log and is undoable. The latest feature test verifies the revlog and source identity after close/reopen. Native conflict-review UI and automatic migration are not claimed complete.

## ADR-011: External Anki package test provenance (M4)

**Status:** independent-public-fixture acceptance verified by Anki feature CI 41320662558; Anki Desktop/AnkiWeb interoperability remains unverified. Retivus downloads without redistributing binaries a public AnkiDroid `update1.apkg` fixture pinned at commit `210a8a2e30be1dc9b21dabb63ae39dc5c1b37963` (Git blob `5a0643b673d44d7e362f3353792460fe5e9b1eca`, documented 1 note and 1 card) and the MIT-licensed public `rhgg2/AnkiTemplate` `Sample deck.apkg` pinned at commit `49013896cabb62608b5c915bcd855bd5f6f419dc` (Git blob `efa7d43b142b626a5b8ca038bcbf0fe5659980b5`). The test must import each into an isolated Retivus-managed profile, compare note/card counts after reexport and reimport, and refuse a malformed subsequent package without erasing existing notes. This does not claim Anki Desktop or AnkiWeb import/export acceptance.

## ADR-012: Offline copy-only collection migration (M4.4)

**Status:** synthetic schema-11/18 migration verified in Anki feature CI 41319176926 and byte-preserving official Anki Desktop collection fixture migration verified in CI 41320662558; M4.1 prerequisite remains open. The original `.anki2` file is never opened through Anki's writing/migrating builder. Journal/WAL/SHM sidecars are refused, the original source file is checked for changes while copying, and only a verified detached copy is migrated and placed under a new Retivus-managed profile. The pinned upstream fixture `rslib/tests/support/mediacheck.anki2` at revision `136bd42bfbd356fc756b6d3e28cc77b02da11ea7` (Git blob `5580db4ef5419c21e76d9f0d31cfe1bb87f88656`) is checked against the same library in CI. This is a database-only path, not live Anki profile access or automatic media-folder migration.

## Pre-AnkiWeb safety artifact (M4.5, pending)

The feature-gated adapter offers `AnkiBackend::create_pre_sync_recovery_bundle` for an **explicitly selected Retivus-managed profile**. It makes an offline, private, no-overwrite ZIP containing a validated SQLite `VACUUM INTO` snapshot, an Anki `.apkg` with *opted-in review history* and referenced media, plus regular raw media files including otherwise-unreferenced files. It rejects symlinks and special media entries, absolute destination paths inside the managed profile, and existing output names; the ZIP is staged privately and published with no-clobber semantics. The API never uses a network client, requests credentials, or modifies the user's normal Anki profile. The recovery bundle must be kept private and restored only to a new profile after separate checks; it does **not** include the media-sync index or establish complete AnkiWeb conflict/media compatibility. Real authenticated login, conflict decisions, progress safety and remote account acceptance remain required for M4.5. No milestone credit is awarded for a backup-only precursor.


## Recovering a private pre-sync bundle into a new profile

**Status: validated backend-only restore, M4.5 still pending.** `AnkiBackend::restore_pre_sync_recovery_as_new_profile(root, profile_id, bundle)` requires a preexisting selected Retivus root, a **new** valid profile ID and an absolute `.zip` recovery bundle. It never opens or overwrites an existing profile, and never authenticates or contacts AnkiWeb.

The restore verifies the raw ZIP central directory independently of `ZipArchive`—including duplicate filenames that the library's entry index can conceal, ZIP64 metadata, only known top-level entries, UTF-8 media paths with no traversal or backslashes, regular-file entries, consistent entry listings and bounded archive sizes. Present limits are 200,000 ZIP members, 128 GiB total declared uncompressed data and 256 MiB central-directory bytes. It reads and CRC-verifies all members, checks the included Anki history `.apkg` structure, and validates the extracted SQLite database with the pinned Anki engine. Only then does it publish the staged database and raw media together in a new private managed-profile directory. It does not import the `.apkg` into the new profile; the database snapshot is the authoritative restore source and the package remains a separate interoperable fallback in the original archive. Existing destination names fail without mutation.

[End-to-end run 41684038693](https://github.com/sguzman/retivus/actions/runs/41684038693) passed **54 Anki library tests**, including an actual recovered/reopened review-history/media round trip, malicious ZIP traversal/unknown-entry refusal and raw duplicate filename rejection. It also passed **28 native GUI tests**. [Foundation 41684038685](https://github.com/sguzman/retivus/actions/runs/41684038685) passed. No original user profiles or credentials were used.

**Limitations:** The snapshot does not include Anki's media-sync index and its source media folder may change concurrently with packaging. Do not share these private archives or treat the tests as evidence of authenticated AnkiWeb compatibility, real conflict resolution or a completed production release. The local recovery API is deliberately not wired to an automatic network-sync action. M4.5 earns no points until its original acceptance requirements are met.
