# Retivus

![Retivus berry-rose OKLCH identity](assets/retivus-color.svg)

**Retivus** is a Rust/egui desktop spaced-repetition application that aims to combine compatibility with the [Anki](https://apps.ankiweb.net/) ecosystem and **source-independent learning** from versioned external datasets.

The first external vocabulary provider is [Mexilex](https://github.com/sguzman/mexilex), a Mexican Spanish lexicon with English definitions. Retivus is not a Mexilex-specific application: future providers will use the same documented, validated import boundary.

## Development status

Early development. The initial repository establishes the Rust/egui application shell, versioned Mexilex and generic TSV validation adapters, and a weighted [implementation roadmap](ROADMAP.md). **An opt-in, CI-tested Anki Rust backend adapter is available; durable collection storage, complete scheduling, synchronization, and deck interchange are not yet implemented or claimed.**

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

An experimental `anki-backend` Cargo feature links the pinned Anki Rust engine and validates an in-memory collection; it is not yet a released backend. The current GUI can read-only validate Mexilex or generic TSV source files. It is a foundation, **not** yet a working flashcard trainer; no learner cards or review history are written.

## License

**Retivus is licensed under AGPL-3.0-or-later.** It was initially intended to use MIT, but we adopted AGPL to support direct reuse of Anki's AGPL-licensed Rust backend in compliance with its terms. **Anki's backend is not yet incorporated into the main application.** Earlier MIT-licensed revisions retain their original terms; see [licensing history and attribution](docs/licensing.md) and [Anki upstream notice](LICENSES/ANKI-ATTRIBUTION.md). External vocabulary datasets and personal study content have separate ownership and licenses.
