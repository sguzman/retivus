# Retivus

![Retivus berry-rose OKLCH identity](assets/retivus-color.svg)

**Retivus** is a Rust/egui desktop spaced-repetition application that aims to combine compatibility with the [Anki](https://apps.ankiweb.net/) ecosystem and **source-independent learning** from versioned external datasets.

The first external vocabulary provider is [Mexilex](https://github.com/sguzman/mexilex), a Mexican Spanish lexicon with English definitions. Retivus is not a Mexilex-specific application: future providers will use the same documented, validated import boundary.

## Development status

Active development; **89% of the weighted acceptance roadmap is verified**. M0–M5 are complete under the owner's explicit **local-only v1** scope; M4 has verified third-party/AnkiDroid packages, cloze/media/CSS round-trips, safe offline collection-copy migration and a real official Anki Desktop GUI import/reexport. **AnkiWeb sync is deferred beyond v1, not validated or needed for local study.** The native learner has verified keyboard/pointer workflows, FSRS reviews, source-conflict guards and X11/Wayland rendering. The 124,444-entry Mexilex release passed bounded-memory and indexed-search benchmarks. Experimental, backend-only tests now verify full-sync refusal, two-client normal collection transfer, timestamp-separated note conflict convergence and media transfer against Anki's pinned *local* Rust server. A reproducible pinned-Anki-server regression confirms that same-second edits to one note can **remain divergent despite successful sync responses**; Retivus preserves both pre-sync local edits in separate private recovery archives, but automated conflict resolution is not implemented. None of these tests establishes production AnkiWeb acceptance. Cross-layer integration and archive security regressions are verified; broader accessibility and release acceptance remain in progress. [ROADMAP.md](ROADMAP.md) is authoritative for verified completion.

## Architecture

Retivus targets a native Rust/egui interface, with System/Light/Dark appearance and explicit persistence boundaries. The preferred Anki interoperability route is a feasibility-tested integration with upstream Anki's Rust `rslib`, rather than an independent reinvention of the Anki collection engine. That feasibility and licensing decision is gated in [the architecture document](docs/architecture.md).

External providers supply *content*; the trainer stores learner-owned decks, cards, review events, and scheduling state separately. A provider update must never silently overwrite learning history or merge ambiguous lexical identities.

The project identity is **berry-rose** (`oklch(69% 0.16 350)`, `#E26DA9`), with contrast-adjusted light and dark accents; see [branding](docs/branding.md). 

See [ROADMAP.md](ROADMAP.md) for phases and measured completion, [docs/product.md](docs/product.md) for scope, [docs/providers.md](docs/providers.md) for the data contract, [Anki Desktop round-trip QA](docs/anki-desktop-qa.md) for the verified independent GUI acceptance, [AGENTS.md](AGENTS.md) for contributor protocol, and [CHANGELOG.md](CHANGELOG.md) for changes.

## Build (development)

Requires a recent stable Rust toolchain and platform dependencies for eframe/egui.

```sh
cargo run
cargo test --all-targets
```

The default `cargo run` build is a **read-only source preview, not the flashcard/study application**. To run local decks, note editing and review sessions, enable the pinned Anki Rust engine's managed-profile learner workspace with:

```sh
cargo run --release --features anki-backend --bin retivus
cargo test --features anki-backend --bin retivus
```

The source validation screen also offers a **read-only word/definition search** after validation. Searches check the full source but retain at most **12 matching records** in the GUI; they never bulk-create flashcards, and changing the file or provider clears stale selections. Validating or importing the full source **never creates cards on its own**. To add Mexilex vocabulary, open a Retivus-owned profile, validate the local `study.jsonl`, then choose **Import verified Mexilex release (no cards)**. Retivus accepts only the exact published `lex-v0.3.0` export, authenticated using its pinned SHA-256 digest, and stages it atomically in that profile's source catalog. A locally regenerated JSONL with the right entry count but a different digest is refused. Select one meaning from the bounded 12-record preview/search, then click **Add selected meaning to Tarium deck**. Only that meaning becomes an Anki card in a dedicated deck such as `Tarium::es-MX::mexilex`; other language/source providers have their own distinct deck names. Existing `Default` cards and review history are not bulk-modified.

To populate a full vocabulary deck, first import the verified Mexilex source catalog, then in the open learner workspace expand **Generate Mexilex first-definition deck**. Preview its 124,444-entry plan, choose the Anki new-card/day and review/day limits, confirm, then click **Generate first-definition deck**. This is a separate explicit action: it creates one card per lexical entry using the first English meaning **in original source order**, tagging each exact source identity. Previously created source-linked cards are skipped across restarts, including cards in another deck. The bounded generation runs incrementally with progress and cancellation; canceling keeps already-created cards, and starting again resumes by skipping those identities. No profile or ordinary Anki collection is opened automatically. The app does not turn all 155,586 distinct senses into cards unless you explicitly add additional meanings individually. First-definition ordering is deterministic, **not** a verified commonness ranking. This opt-in bulk feature is still subject to real-workstation performance/UX QA before release.

The learner workspace never automatically opens your ordinary Anki profile. Choose an existing profile root and a separate Retivus profile ID, then explicitly create/open it. Review grades and undo use the pinned Anki scheduler. Package import/export accepts explicitly chosen absolute `.apkg` paths, checks archives before importing, and refuses export overwrites. Imports and exports each exclude personal review/scheduling history by default, with separate explicit opt-ins. Generic TSV content can be staged into the profile's SQLite source catalog. Mexilex raw JSONL remains **preview-only until** the immutable published `lex-v0.3.0` digest has been verified by the explicit catalog import action. The import itself creates no learner cards. Source identities are never guessed from word spellings. Rendered card HTML is currently shown as inert text, not executed as a web page. This is a development feature, **not** a finished release or AnkiWeb sync client.

## License

**Retivus is licensed under AGPL-3.0-or-later.** It was initially intended to use MIT, but we adopted AGPL to support direct reuse of Anki's AGPL-licensed Rust backend in compliance with its terms. **Anki's backend is linked only when the experimental `anki-backend` Cargo feature is enabled.** Earlier MIT-licensed revisions retain their original terms; see [licensing history and attribution](docs/licensing.md) and [Anki upstream notice](LICENSES/ANKI-ATTRIBUTION.md). External vocabulary datasets and personal study content have separate ownership and licenses.
