# Anki Rust backend feasibility spike

## Pinned research target

- Upstream repository: https://github.com/ankitects/anki
- Commit: `136bd42bfbd356fc756b6d3e28cc77b02da11ea7`
- Requested upstream toolchain: Rust **1.99.0**, per `rust-toolchain.toml` at the research target.
- Crate: `rslib/Cargo.toml`, package `anki`; `publish = false`, `workspace = ".."`.
- License: upstream **AGPL-3.0-or-later**. Retivus has since proactively adopted AGPL-3.0-or-later too, although no linked main-app Anki integration has been released. See [licensing history](licensing.md).
- Entry API: public `anki::collection::CollectionBuilder` in `rslib/src/collection/mod.rs`; builder can target disposable `:memory:` collection.
- Backend public Rust modules: `collection`, `card`, `notes`, `decks`, `scheduler`, `import_export`, `sync`, `media`, `notetype` and `storage`.

The pinned root `Cargo.toml` defines a 12-member Rust workspace, including the `rslib`, `rslib/i18n`, `rslib/io`, `rslib/proto`, `rslib/sync`, `rslib/process`, `rslib/linkchecker`, `pylib/rsbridge`, and build-support packages. `rslib` depends on workspace path crates `anki_i18n`, `anki_io`, `anki_proto`, and `anki_proto_gen`. Upstream dependencies include `fsrs = 6.6.2`, a pinned `unicase = 2.6.0`, and pinned Git revisions of `ankitects/rust-url` and `ankitects/linkcheck` in the workspace. The protobuf crate invokes `prost-build` and needs a system `protoc`; the CI spike installs `protobuf-compiler` and sets `PROTOC=/usr/bin/protoc` explicitly. The normal Retivus UI and provider validator are independent of this experimental backend.

This is an **inventory**, not proof of a portable API. The workspace has substantial internal dependencies and build-time generated protobuf and internationalization assets.

## Defensive package preflight (pending feature acceptance)

The native Anki-package importer checks that the selected absolute .apkg is a regular, non-symlinked file. Before passing it to pinned Anki, Retivus inspects ZIP central-directory metadata without extracting files. It rejects absolute and traversal paths, Windows path syntax, control characters, archive symlinks, and suspicious declared expansion ratios. It caps archive entries at one million and declared expanded data at 64 GiB. Safe Unicode media paths remain permitted.

Disposable-profile tests construct malicious packages and verify rejection without altering existing note fields or writing outside the selected profile. This check supplements upstream Anki's importer and is not a replacement ZIP extractor or a guarantee against every malicious archive. M6.3 remains pending until feature tests pass. AnkiWeb credentials and sync are not used by this offline import path.

## Repeatable build experiment

[Anki Rust integration feasibility workflow](../.github/workflows/anki-backend-spike.yml) checks out the exact upstream commit into an isolated temporary directory and attempts `cargo check --locked -p anki` on GitHub's Linux worker. It never writes to the Retivus source tree, modifies Anki, or opens a user's collection. An experiment failure must be documented verbatim, classified (toolchain/dependency/build-script/API/license), and resolved before any claim of successful direct integration.

### Success criteria

1. CI builds pinned backend workspace; archive a concise log.
2. A **separate crate** in Retivus builds against that checkout using a path dependency, opens a disposable Anki collection, tests a real note/card and review queue, and leaves no data on disk. This is a separate, still-pending acceptance test.
3. Evaluate binary dependencies and size, codegen, upstream update costs, and AGPL obligations before choosing the final adapter.
4. Retain an explicit fallback study: Anki native backend service/protobuf or compatible export/import if direct embedding is unworkable. Avoid reimplementation before measuring this gate.

No Anki API, package, scheduling, synchronization or add-on compatibility is claimed at this stage.

## Observed build incompatibility (2026-11-01)

The first independent pinned build failed because `protoc` was not available; after installing it, the next attempt needed the two FTL translation submodules `ftl/core-repo` and `ftl/qt-repo`. With both resolved, `cargo check --locked -p anki` reached upstream Rust sources but failed because the workspace's `tokio` dependency lacks `io-util`, while `rslib` imports Tokio's async read/write extension traits and buffered I/O. The test workflow **does not commit any Anki source changes**: it explicitly adds only the `io-util` feature to a disposable checkout of the pinned `Cargo.toml`, displays that diff and retries. This is a feasibility workaround, not a claim that upstream builds unmodified as an independent package. If necessary, upstream integration will need a patch, an upstream issue, or a different adapter. No released Retivus binary includes this upstream code yet.

## Verified backend compilation (2026-11-01)

[Workflow 41262898809](https://github.com/sguzman/retivus/actions/runs/41262898809) **succeeded** using the exact pinned Anki source revision. The CI checkout included upstream FTL submodules, `protobuf-compiler`, and a separately displayed, isolated `tokio/io-util` feature adjustment in the temporary Anki manifest. This proves the underlying backend can compile on the Linux runner with that explicit integration workaround; it is not yet proof an external app can use the API. The next [separate smoke crate](../spikes/anki-collection/README.md) tests a throwaway in-memory collection and review queue, without importing user data or distributing upstream code.

## Independent external-client feasibility verified

[Anki integration run 41266034216](https://github.com/sguzman/retivus/actions/runs/41266034216) succeeded for the standalone external Rust client: `CollectionBuilder` opened disposable in-memory storage, a deck and Basic note were added, a card was found, and that card appeared in Anki's scheduled queue. Upstream Card's identifier is accessed through its **public `id()` method**, not its private field. This verifies source-level integration without Python/Qt; it does not demonstrate direct integration into the shipping UI, AnkiWeb sync or app packaging.

A second experiment in `spikes/anki-git/` tests direct pinned Cargo Git consumption, using Cargo feature unification for Tokio `io-util`. [Direct pinned Cargo Git run 41266535589](https://github.com/sguzman/retivus/actions/runs/41266535589) succeeded. [Retivus feature-enabled backend CI 41266965280](https://github.com/sguzman/retivus/actions/runs/41266965280) also passed the actual library wrapper tests.
