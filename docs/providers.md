# External data providers

Retivus is not a Mexilex GUI. The first importer validates the existing **Mexilex study JSONL format v1**, and a generic internal provider abstraction is planned for later formats.

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

## Future providers

A provider adapter must describe its format/version and licensing, parse and validate into source-neutral records, expose provenance and snapshot identity, and present a reversible mapping to note/card generation. An unknown format is rejected with a useful error, not guessed from filenames. Contract tests with a second genuinely different format are mandatory.
