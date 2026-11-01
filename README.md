# Retivus

![Retivus berry-rose OKLCH identity](assets/retivus-color.svg)

**Retivus** is a Rust/egui desktop spaced-repetition application that aims to combine compatibility with the [Anki](https://apps.ankiweb.net/) ecosystem and **source-independent learning** from versioned external datasets.

The first external vocabulary provider is [Mexilex](https://github.com/sguzman/mexilex), a Mexican Spanish lexicon with English definitions. Retivus is not a Mexilex-specific application: future providers will use the same documented, validated import boundary.

## Development status

Active development. The [weighted implementation roadmap](ROADMAP.md) records only verified task completions. M0–M3 are verified, including Anki-backed FSRS grading and local recovery. The opt-in native learner workspace has verified real Anki due-queue review, grading, undo and persistence on X11, with managed-profile notes, source selection, and package import/export. Complete keyboard-and-mouse import-to-review UX, source-conflict management, native Wayland QA, independent Anki Desktop/AnkiWeb interoperability and release acceptance remain in progress.

## Architecture

Retivus targets a native Rust/egui interface, with System/Light/Dark appearance and explicit persistence boundaries. The preferred Anki interoperability route is a feasibility-tested integration with upstream Anki's Rust `rslib`, rather than an independent reinvention of the Anki collection engine. That feasibility and licensing decision is gated in [the architecture document](docs/architecture.md).

External providers supply *content*; the trainer stores learner-owned decks, cards, review events, and scheduling state separately. A provider update must never silently overwrite learning history or merge ambiguous lexical identities.

The project identity is **berry-rose** (`oklch(69% 0.16 350)`, `#E26DA9`), with contrast-adjusted light and dark accents; see [branding](docs/branding.md). 

See [ROADMAP.md](ROADMAP.md) for phases and measured completion, [docs/product.md](docs/product.md) for scope, [docs/providers.md](docs/providers.md) for the data contract, [AGENTS.md](AGENTS.md) for contributor protocol, and [CHANGELOG.md](CHANGELOG.md) for changes.

## Build (development)

Requires a recent stable Rust toolchain and platform dependencies for eframe/egui.

```sh
cargo run
cargo test --all-targets
```

The default `cargo run` build remains a **read-only source preview**. Opt into the pinned Anki Rust engine and its experimental managed-profile learner workspace with:

```sh
cargo run --features anki-backend --bin retivus
cargo test --features anki-backend --bin retivus
```

The learner workspace never automatically opens your ordinary Anki profile. Choose an existing profile root and a separate Retivus profile ID, then explicitly create/open it. Review grades and undo use the pinned Anki scheduler. Package import/export accepts explicitly chosen absolute `.apkg` paths and refuses export overwrites. The preview can stage exact senses from a user-described generic TSV source into the profile's SQLite source catalog; raw Mexilex JSONL is **preview-only** until its release archive has been independently checksum/rights-verified and imported into that catalog. Source identities are never guessed from word spellings. Rendered card HTML is currently shown as inert text, not executed as a web page. This is a development feature, **not** a finished release or AnkiWeb sync client.

## License

**Retivus is licensed under AGPL-3.0-or-later.** It was initially intended to use MIT, but we adopted AGPL to support direct reuse of Anki's AGPL-licensed Rust backend in compliance with its terms. **Anki's backend is linked only when the experimental `anki-backend` Cargo feature is enabled.** Earlier MIT-licensed revisions retain their original terms; see [licensing history and attribution](docs/licensing.md) and [Anki upstream notice](LICENSES/ANKI-ATTRIBUTION.md). External vocabulary datasets and personal study content have separate ownership and licenses.
