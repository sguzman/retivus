# Anki Rust backend feasibility spike

## Pinned research target

- Upstream repository: https://github.com/ankitects/anki
- Commit: `136bd42bfbd356fc756b6d3e28cc77b02da11ea7`
- Requested upstream toolchain: Rust **1.99.0**, per `rust-toolchain.toml` at the research target.
- Crate: `rslib/Cargo.toml`, package `anki`; `publish = false`, `workspace = ".."`.
- License: upstream **AGPL-3.0-or-later**, which is not automatically compatible with distributing Retivus as solely MIT if linked.
- Entry API: public `anki::collection::CollectionBuilder` in `rslib/src/collection/mod.rs`; builder can target disposable `:memory:` collection.
- Backend public Rust modules: `collection`, `card`, `notes`, `decks`, `scheduler`, `import_export`, `sync`, `media`, `notetype` and `storage`.

This is an **inventory**, not proof of a portable API. The workspace has substantial internal dependencies and build-time generated protobuf and internationalization assets.

## Repeatable build experiment

[Anki Rust integration feasibility workflow](../.github/workflows/anki-backend-spike.yml) checks out the exact upstream commit into an isolated temporary directory and attempts `cargo check --locked -p anki` on GitHub's Linux worker. It never writes to the Retivus source tree, modifies Anki, or opens a user's collection. An experiment failure must be documented verbatim, classified (toolchain/dependency/build-script/API/license), and resolved before any claim of successful direct integration.

### Success criteria

1. CI builds pinned backend workspace; archive a concise log.
2. A **separate crate** in Retivus builds against that checkout using a path dependency, opens a disposable Anki collection, tests a real note/card and review queue, and leaves no data on disk. This is a separate, still-pending acceptance test.
3. Evaluate binary dependencies and size, codegen, upstream update costs, and AGPL obligations before choosing the final adapter.
4. Retain an explicit fallback study: Anki native backend service/protobuf or compatible export/import if direct embedding is unworkable. Avoid reimplementation before measuring this gate.

No Anki API, package, scheduling, synchronization or add-on compatibility is claimed at this stage.
