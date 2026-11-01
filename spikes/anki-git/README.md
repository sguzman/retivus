# Alternative pinned Cargo Git dependency experiment

This AGPL-3.0-or-later research-only crate tests whether `anki` can be resolved directly by Cargo from the exact upstream Anki commit (`136bd42bfbd356fc756b6d3e28cc77b02da11ea7`). It requests Tokio's `io-util` feature via Cargo feature unification, **without modifying upstream Cargo.toml**.

The [CI workflow](../../.github/workflows/anki-backend-spike.yml) builds and runs the same isolated in-memory collection/note/card/review-queue smoke used by the path-dependency experiment. Success would demonstrate a maintainable Git dependency route; it would **not** establish all collection compatibility, profile safety, full frontend parity, or AnkiWeb sync.

No Retivus shipping package depends on this experiment yet. This crate must not open existing user collections or credentials.
