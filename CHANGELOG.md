# Changelog

All meaningful product changes will be recorded here. Detailed implementation status belongs in [ROADMAP.md](ROADMAP.md), not the README.

## Unreleased

### Added
- Added managed-only `.apkg` import/export with media, template, tag and card round trips; pinned third-party AnkiDroid and MIT sample packages passed independent checks (Anki Desktop GUI interoperability still pending).
- Added cloze/HTML/CSS/SVG render parity tests across full `.apkg` export/import and reopen.
- Added offline `.anki2` copy-only migration with schema-checking, source-byte preservation, active WAL refusal, and official upstream Anki Desktop fixture verification.
- Added persisted 80–150% native UI scale, compact/comfortable density and responsive toolbar/input sizing; full accessibility QA pending.
- Verified explicit, bounded source snapshot diffs and opt-in reversible identity migrations, with Anki note updates that preserve existing review history and refuse manual-edit conflicts.
- Verified Anki FSRS four-rating transitions, due queue restart, review persistence and undo using the pinned upstream scheduler.
- Added explicit sense-specific source card candidates and Anki note creation with immutable provenance tags; homographs and multiple senses retain distinct identities.
- Added non-clobber managed-profile SQLite snapshots and restore-as-new recovery, verified through pinned Anki integrity checks and isolated feature-CI regression tests.
- Added undoable deck/note-type/template editing and guarded destructive deck deletion in the feature-gated Anki adapter.
- Optional local Krypton Nerd Font discovery and validated loading, with native fallback and no bundled font assets.
- Verified independent Japanese-to-Spanish TSV provider through a standalone CLI, persistent catalog retrieval, source-identity separation, and GUI-compatible preview contract tests.
- Added Anki-backed deck/notetype enumeration, bidirectional templates, note field CRUD and undo, with persistent disposable integration tests.
- Added transactional SQLite source-snapshot catalog with rollback, duplicate and persistent-reopen tests, plus full real Mexilex study-import CI.
- Introduced restricted Retivus-owned Anki profile create/reopen APIs and disposable schema migration, isolation, symlink and unsupported-future-version regression tests.
- Added an opt-in, pinned Anki Git backend module for in-memory deck/note/queue operations and isolated library tests.
- Bounded source-neutral vocabulary preview in the native egui validation UI, with late-error rollback tests.
- Verified pinned Anki Rust backend external-client deck/note/card/review-queue interoperability in isolated CI.
- Added read-only egui source format selection for Mexilex and generic provider TSV.
- Verified persistent UI appearance and light/dark OS-following theme behavior.
- Source-neutral adapter interface and a genuinely distinct versioned TSV provider, with independent language/licensing metadata and contract tests.
- Persistent System/Light/Dark theme preferences, restored automatically at app launch.
- Retivus OKLCH berry-rose identity, verified light/dark accent tokens and a shared egui color module.
- Initial Rust/egui desktop shell with explicit System, Light and Dark appearance.
- First strict, streaming Mexilex JSONL contract validator and regression tests.
- Product scope, weighted milestone roadmap, Anki Rust backend feasibility gate, generic-provider architecture and contributor protocol.

### Changed
- Adopted AGPL-3.0-or-later for the project in anticipation of direct Anki Rust backend integration; retained the original MIT grant and documented the provenance and remaining compliance gates.

### Known limitations
- Neither selected-source note generation nor Anki grading is exposed in the shipping GUI yet; database-only backups do not cover media. Anki package import/export, full power-failure testing, and AnkiWeb interoperability remain incomplete.
- Pinned Anki engine feasibility is verified, but release-grade corresponding-source notices, packaging, and distribution compliance remain unfinished.
