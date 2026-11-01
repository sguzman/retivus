# Retivus

**Retivus** is a Rust/egui desktop spaced-repetition application that aims to combine compatibility with the [Anki](https://apps.ankiweb.net/) ecosystem and **source-independent learning** from versioned external datasets.

The first external vocabulary provider is [Mexilex](https://github.com/sguzman/mexilex), a Mexican Spanish lexicon with English definitions. Retivus is not a Mexilex-specific application: future providers will use the same documented, validated import boundary.

## Development status

Early development. The initial repository establishes the Rust/egui application shell, a versioned source-record validation boundary, and a weighted [implementation roadmap](ROADMAP.md). **Anki backend integration, native collection access, scheduling, synchronization, and deck interchange are not yet implemented or claimed.**

## Architecture

Retivus targets a native Rust/egui interface, with System/Light/Dark appearance and explicit persistence boundaries. The preferred Anki interoperability route is a feasibility-tested integration with upstream Anki's Rust `rslib`, rather than an independent reinvention of the Anki collection engine. That feasibility and licensing decision is gated in [the architecture document](docs/architecture.md).

External providers supply *content*; the trainer stores learner-owned decks, cards, review events, and scheduling state separately. A provider update must never silently overwrite learning history or merge ambiguous lexical identities.

See [ROADMAP.md](ROADMAP.md) for phases and measured completion, [docs/product.md](docs/product.md) for scope, [docs/providers.md](docs/providers.md) for the data contract, [AGENTS.md](AGENTS.md) for contributor protocol, and [CHANGELOG.md](CHANGELOG.md) for changes.

## Build (development)

Requires a recent stable Rust toolchain and platform dependencies for eframe/egui.

```sh
cargo run
cargo test --all-targets
```

The first GUI is a foundation, **not** a working flashcard trainer.

## License

This repository presently retains its original MIT license. Upstream Anki `rslib` uses **AGPL-3.0-or-later**; adopting that code requires a reviewed project licensing decision and compatible redistribution before it is linked or vendored. The two codebases have **not** been combined. Lexical data has its own upstream license and is not covered by this repository's MIT license.
