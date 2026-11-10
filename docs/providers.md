# External data providers

Retivus is not a Mexilex GUI. The first importer validates the existing **Mexilex study JSONL format v1**. A source-neutral `SourceAdapter` interface supports multiple versioned formats, and the egui validation screen exposes both Mexilex and generic TSV sources.

## Mexilex v0.3.0

Use the verified [lex-v0.3.0 study release](https://github.com/sguzman/mexilex/releases/tag/lex-v0.3.0), notably `mexilex-study-v0.3.0.tar.gz`. Verify public SHA256SUMS and bundled manifests before importing. `study.jsonl` has **124,444** non-inflection-only lexical entries, with `mexico-labeled.jsonl` as a separate subset.

Each JSONL entry includes:
- `format_version = 1`, `source = "mexilex"`, `source_edition`, `lexical_language = "es-MX"`, `definition_language = "en"`;
- a content-derived `entry_id`, `word`, `part_of_speech`, optional `etymology_number`, `forms`, `senses`, `mexico_labeled`, `inflection_only`;
- each sense has its own `id`, `definition_en` and optional regional flags, examples, and labels.

Source IDs are not permanent learner card IDs. The same spelling can have several senses and parts of speech. The exporter filters pure inflections by default; a trainer can request an all-forms export when appropriate.

Other evidence (ACTIV-ES film dialogue frequency, CIEMPIESS radio frequencies, Wikcionario regional spellings) is in **separately licensed sidecars**. Surface frequency is not a license to claim Mexico-specific meanings. Retivus must not silently merge GPL, CC BY-SA, GFDL and proprietary media.

## Import validation v0 (initial foundation)

`src/provider.rs` provides a strict streaming boundary that accepts one JSON record per line, checks required version/language/source/IDs/word/POS/senses, rejects repeated entries and sense IDs, and returns source records without creating cards. It is an intentionally **partial** importer: it does not yet verify downloaded checksums, archive manifests, transactionally install into a profile, or update existing study cards. Those are separately tracked milestones.

## Source-neutral adapters (implementation)

`src/source_adapter.rs` exposes a stable read-only abstraction, `SourceAdapter`, with:
- explicit `SourceInfo` for provider ID, edition, lexical/definition language pair and license notice;
- `SourceRecord` / `SourceMeaning` independent of Mexilex's schema;
- an object-safe `visit` method that streams validated records to a caller-supplied staging/preview sink, never directly to the learner's personal cards;
- two independent formats: `mexilex-study-jsonl-v1` and `retivus-study-tsv-v1`.

The explicit TSV v1 format is a minimal second provider used to verify the interface without coupling it to Mexilex. It requires a UTF-8 header `entry_id<TAB>word<TAB>part_of_speech<TAB>sense_id<TAB>definition` and one complete entry/sense per line. Repeated spellings with distinct IDs are accepted; duplicate IDs and missing fields are refused. The source descriptor is provided externally, not guessed from filename. A sample source might use Japanese words with Spanish meanings, distinct from Mexilex's es-MX/en pair. Do not treat this format as the final importer, and do not interpret a field as a license for republishing content.

**Security/transaction limit:** adapters validate streaming records and can emit earlier records before a later invalid one is discovered. Callers **must stage and roll back** the entire import on failure. Validation alone is not permission to change cards or review history.

The default native GUI supports bounded read-only validation and search for either format; selecting TSV requires explicit source ID, edition, language pair and license metadata. In the **opt-in Anki-backed GUI**, the user must open a Retivus-owned profile and explicitly choose a single source meaning before creating one note. The add operation checks the candidate against the imported immutable SQLite snapshot and preserves distinct homograph/sense IDs. Selected cards go to per-language, per-provider decks such as `Tarium::es-MX::mexilex`; the `Default` deck is not used for source cards.

For Mexilex specifically, **Import verified Mexilex release (no cards)** checks the local `study.jsonl` against the pinned SHA-256 of the *uncompressed* official `lex-v0.3.0` file (`c4e3f721a9f4004645ff7e1adfa5c628bfee870d0f46975732baafed03f3ea86`). The digest was recorded by the official-package acceptance workflow after verifying upstream `SHA256SUMS` and the release manifest. The file is copied to temporary storage and hashed before a catalog transaction inserts the snapshot and verification attestation atomically. Reimporting the same attested snapshot is idempotent. An unverified or locally altered export is rejected before changing the profile catalog; ordinary low-level imports do not carry verification attestation and cannot masquerade as the official snapshot. Importing the source catalog creates **zero cards** and does not touch Anki review history. Clicking **Add selected meaning to Tarium deck** is a separate explicit learner action. There is also a separate opt-in **Generate Mexilex first-definition deck** action, available only after the same authenticated full catalog is installed, previewed and confirmed. It pages the catalog by stable entry ID, uses the stored original first sense (rather than lexicographic sense-ID order), and writes at most one first-meaning note per lexical entry, skipping all notes with the same exact source tag. The Anki deck's native per-deck new/review daily caps are configurable. It creates in bounded GUI-driven batches, retains committed cards on cancel and reindexes their tags when restarted. No automatic background mutation, delete or bulk rewrite of existing learner history occurs. The source-upgrade/conflict UX remains separate work.

## Future providers

A provider adapter must describe its format/version and licensing, parse and validate into source-neutral records, expose provenance and snapshot identity, and present a reversible mapping to note/card generation. An unknown format is rejected with a useful error, not guessed from filenames. Contract tests with a second genuinely different format are mandatory.

## Atomic source catalog import

The source-neutral `SourceCatalog` stores externally provided records, sense IDs, snapshot metadata and license notices in a separate SQLite database. Its single transaction per snapshot guarantees that a late JSONL error, a duplicate source key, or a failed SQL constraint **rolls back the entire new snapshot**, leaving existing snapshots intact. The catalog enforces unique provider/snapshot/entry identities and sense identities. Multiple providers and historical snapshots coexist without mixing user-owned cards or reviews.

`retivus import-source` is exposed as `cargo run --bin import-source -- <verified-study.jsonl> <new-catalog.sqlite3> <snapshot>`. The target catalog is create-only, refusing existing files. The source JSONL **must already have passed release archive checksum and license validation**, which the CI study-package workflow performs before invoking the binary. An immutable snapshot key must be supplied explicitly and should correspond to the verified release. The catalog does not independently authenticate the supplied label, and no user learning history is modified.

This is an import/staging foundation, not a complete provider lifecycle: manifest signature verification, user-initiated snapshot upgrades, review-history reconciliation, and note/card generation are still separate tasks.

## Full-release preview and import memory gate

The provider acceptance workflow benchmarks the **entire verified 124,444-entry Mexilex release**, not a small synthetic excerpt. A dedicated Rust binary streams the records through the exact `validate_preview` function used by the native UI and asserts that it retains at most **12** lexical records while still validating all source entries. The importer writes the independent SQLite catalog without touching any private learner profile.

The CI records peak resident-set memory (RSS) for the preview, catalog import and **1,000 indexed source lookups** separately using GNU `time`; each is required to stay below **512 MiB** on the Linux runner. The lookup benchmark also has a 10-second upper bound. The output is retained in the workflow summary. This is a measured bound for the pinned release on the test runner, **not** a general proof that all future source formats or malicious inputs have constant memory use. The performance acceptance remains pending until the workflow executes successfully.

## Verified public-release ingestion

The [2026-11-01 integration run](https://github.com/sguzman/retivus/actions/runs/41268497201) checked the public Mexilex v0.3.0 release and successfully committed **124,444 entries and 155,586 senses** to the separate SQLite catalog. The [Rust regression run](https://github.com/sguzman/retivus/actions/runs/41268551513) also verified rollback after late input failure and refusal to overwrite an existing snapshot. This is a source ingestion milestone, not a deck/card import or learner-history migration.

## Independent second-provider acceptance

The `import-generic` executable accepts a versioned TSV and **explicit** provider ID, edition, lexical/definition language tags and license notice. It writes a new, independent SQLite source catalog; it never opens or touches an Anki profile. The cross-format fixture `tests/fixtures/independent-ja-es.tsv` contains original Japanese-to-Spanish sample records, including homographs with distinct entry IDs. `tests/generic_provider.rs` exercises bounded previews, a disk-backed import, reopening, immutable identity and sense/rights provenance. The `Rust foundation` workflow also runs a real standalone CLI import and verifies row/sense counts. This is an integration acceptance test for a second format; it does **not** create flashcards or validate arbitrary third-party license claims.

## Experimental selected-sense card bridge

The feature-gated native learner interface now exposes a **non-card-creating** staging action for fully validated generic TSV snapshots, using the active Retivus profile's pinned SQLite catalog. Duplicate snapshot keys are refused and the entire import is transactional. Raw Mexilex sources remain blocked from this action: only verified-release ingestion can populate the Mexilex catalog. A selected learner note can be explicitly compared with its immutable provider/snapshot/entry/sense identity and reviewed for manual edits or an already recorded one-to-one mapping. The native learner workspace also exposes paged source-only snapshot comparisons (50 changes per page), identifying changed, added, and removed exact entry/sense IDs without conflating homographs or loading private review state. Anki note changes never follow a diff automatically.

The feature-gated learner GUI additionally provides an explicit one-to-one mapping editor: the user specifies a target snapshot, entry and sense, previews its exact content/license, approves recording the mapping without changing notes, and then separately chooses whether to apply that recorded mapping to a selected note. The pinned Anki backend independently guards against manual edits and ambiguous provenance, preserves card/review identity, and supports undo. The catalog's recorded mapping can also be reversed independently before applying it to a note. It does not automatically migrate all notes. GUI end-to-end acceptance is under CI verification.

The source catalog's `lookup_entry(provider, snapshot, entry_id)` restores a precise published entry with its individually identified senses and attribution. `select_sense(provider, snapshot, entry_id, sense_id)` returns a selected `SourceCardCandidate` without creating a learner record. The candidate's reversible `retivus::source` tag encodes provider/snapshot/entry/sense separately as hex UTF-8, so identical spellings do not collide. The feature-gated Anki adapter can then explicitly create a Basic note using that choice, retaining both the user's new Anki note identity and source provenance on a separate plane. Test fixtures cover Japanese homographs with distinct source entries and two Mexilex meanings under the same entry.

The opt-in feature-enabled learner GUI now exposes exact sense selection and explicit note creation in a managed profile, with immutable source identities checked against its SQLite catalog. When a note is selected in the learner browser, its reversible provider/snapshot/entry/sense identity is displayed; malformed or multiply assigned Retivus provenance tags are flagged rather than silently reconciled. This integration is under acceptance testing and does not automatically import the entire catalog, reconcile updates, deduplicate repeated choices, or change any existing review history.

## Verified explicit-sense and second-provider acceptance — 2026-11-01

[Native Rust CI 41309537553](https://github.com/sguzman/retivus/actions/runs/41309537553) passed the independent Japanese-to-Spanish TSV import, disk-backed lookup, bounded native preview and sense-choice tests. [Anki feature CI 41309537491](https://github.com/sguzman/retivus/actions/runs/41309537491) passed 27 tests, including selecting two homographic but semantically distinct entries into separate Anki-owned notes, grading a queued review and reopening their review history. The source neutral provider engine was not specialized to Japanese, Spanish or Mexilex. These results complete M3.3 and M3.5; they do **not** establish production deck browsing, Anki package/media round-trips or automatic version reconciliation.

## Experimental version reconciliation (M3.4)

The source catalog compares immutable snapshots in pages of at most 1,000 exact entry/sense identities. Added or removed homographs are surfaced as distinct changes, not guessed from identical spellings. Explicit user-approved, one-to-one mappings between selected source identities are durable in the source SQLite catalog and reversible without altering private review history. The feature-gated Anki adapter has **passed** guarded updates to an **existing** note in [CI 41313808023](https://github.com/sguzman/retivus/actions/runs/41313808023). It rejects manually edited field content, multiple source tags, missing mapping or stale provider metadata, retaining note/card identity and review events through undo/reopen. This completes the M3.4 backend acceptance, **not** unattended automatic migration or completed UI conflict resolution.
