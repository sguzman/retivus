# Changelog

All meaningful product changes will be recorded here. Detailed implementation status belongs in [ROADMAP.md](ROADMAP.md), not the README.

## Unreleased

### Added
- Added opt-in generic TSV snapshot staging, immutable note provenance inspection and a two-step source revision UI (exact target preview/approval followed by separately guarded Anki note migration); feature CI acceptance is pending.
- Added true persisted collection statistics, bounded source-linked/plain/conflicting-note browser filters, and guarded note deletion with Anki undo. New feature-level regression verification is pending.
- Added a complete validation → precise source-sense choice → real Anki card → grade → restart regression through the same handlers used by the native GUI, plus immutable active-profile identity for source catalog isolation; feature-level regression verification is pending.
- Verified native selected-deck scheduling, reveal, guarded grading, undo, due counts and review history across restart in [Anki CI 41329550912](https://github.com/sguzman/retivus/actions/runs/41329550912).
- Added opt-in learner UI action to archive an exact selected generic source sense in a managed SQLite snapshot and explicitly create an Anki note, rejecting forged meanings and refusing raw unverified Mexilex imports; acceptance tests pending.
- Added managed-only `.apkg` import/export with media, template, tag and card round trips; pinned third-party AnkiDroid and MIT sample packages passed independent checks (Anki Desktop GUI interoperability still pending).
- Added cloze/HTML/CSS/SVG render parity tests across full `.apkg` export/import and reopen.
- Added offline `.anki2` copy-only migration with schema-checking, source-byte preservation, active WAL refusal, and official upstream Anki Desktop fixture verification.
- Verified persisted UI scale and density with actual 680×560/1100×760 native X11 screenshots and keyboard-focus smoke; Wayland and full accessibility QA remain separately scoped.
- Added feature-gated learner profiles, deck/note editing, bounded Anki search, `.apkg` operations, and due-card reveal/grading/undo controls; full native-flow acceptance pending.
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
- The opt-in native learner workspace is not yet release-validated. Rich Anki card HTML is displayed as inert source text, not rendered. Database-only backups do not cover media; independent Anki Desktop/AnkiWeb interoperability and full power-failure testing remain unfinished.
- Pinned Anki engine feasibility is verified, but release-grade corresponding-source notices, packaging, and distribution compliance remain unfinished.
