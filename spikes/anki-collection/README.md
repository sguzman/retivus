# Disposable Anki backend integration probe

**Not part of the shipping Retivus binary.** This is an AGPL-3.0-or-later compatibility experiment that links the exact pinned upstream Anki Rust package *inside CI only*.

The companion [workflow](../../.github/workflows/anki-backend-spike.yml) checks out Anki into `upstream-anki/`, initializes its required translation submodules, installs `protoc`, applies an explicitly documented `tokio/io-util` feature change **only inside that ephemeral checkout**, and builds the Rust backend before running this client. No real Anki collection, profile or credentials are read.

The client exercises creation of an in-memory collection, deck, Basic note, generated card, search, and next-card queue. This is a feasibility gate for future Retivus integration, not a release, AnkiWeb sync, deck package importer or third-party add-on compatibility claim.
