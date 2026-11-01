# Changelog

All meaningful product changes will be recorded here. Detailed implementation status belongs in [ROADMAP.md](ROADMAP.md), not the README.

## Unreleased

### Added
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
- Persistent System/Light/Dark theme preferences, restored automatically at app launch.\n- Retivus OKLCH berry-rose identity, verified light/dark accent tokens and a shared egui color module.
- Initial Rust/egui desktop shell with explicit System, Light and Dark appearance.
- First strict, streaming Mexilex JSONL contract validator and regression tests.
- Product scope, weighted milestone roadmap, Anki Rust backend feasibility gate, generic-provider architecture and contributor protocol.

### Changed
- Adopted AGPL-3.0-or-later for the project in anticipation of direct Anki Rust backend integration; retained the original MIT grant and documented the provenance and remaining compliance gates.

### Known limitations
- Does not yet import a release into persistent collections, schedule cards, open Anki packages or synchronize with AnkiWeb.
- Anki backend adoption and corresponding licensing requirements remain unverified.
