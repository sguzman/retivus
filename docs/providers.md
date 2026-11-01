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

The native GUI supports read-only validation for either format; selecting TSV requires the operator to enter explicit source ID, edition, language pair and license metadata. The validation result **never creates cards**. A profile-backed transactional importer and real external-provider upgrade handling are still pending.

## Future providers

A provider adapter must describe its format/version and licensing, parse and validate into source-neutral records, expose provenance and snapshot identity, and present a reversible mapping to note/card generation. An unknown format is rejected with a useful error, not guessed from filenames. Contract tests with a second genuinely different format are mandatory.

## Atomic source catalog import

The source-neutral `SourceCatalog` stores externally provided records, sense IDs, snapshot metadata and license notices in a separate SQLite database. Its single transaction per snapshot guarantees that a late JSONL error, a duplicate source key, or a failed SQL constraint **rolls back the entire new snapshot**, leaving existing snapshots intact. The catalog enforces unique provider/snapshot/entry identities and sense identities. Multiple providers and historical snapshots coexist without mixing user-owned cards or reviews.

`retivus import-source` is exposed as `cargo run --bin import-source -- <verified-study.jsonl> <new-catalog.sqlite3> <snapshot>`. The target catalog is create-only, refusing existing files. The source JSONL **must already have passed release archive checksum and license validation**, which the CI study-package workflow performs before invoking the binary. An immutable snapshot key must be supplied explicitly and should correspond to the verified release. The catalog does not independently authenticate the supplied label, and no user learning history is modified.

This is an import/staging foundation, not a complete provider lifecycle: manifest signature verification, user-initiated snapshot upgrades, review-history reconciliation, and note/card generation are still separate tasks.

## Verified public-release ingestion

The [2026-11-01 integration run](https://github.com/sguzman/retivus/actions/runs/41268497201) checked the public Mexilex v0.3.0 release and successfully committed **124,444 entries and 155,586 senses** to the separate SQLite catalog. The [Rust regression run](https://github.com/sguzman/retivus/actions/runs/41268551513) also verified rollback after late input failure and refusal to overwrite an existing snapshot. This is a source ingestion milestone, not a deck/card import or learner-history migration.
