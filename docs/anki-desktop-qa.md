# Anki Desktop GUI round-trip QA — M4.2

**Status:** not accepted. This workflow creates a test fixture with Retivus, then requires a real human/GUI or independently automated Anki Desktop importer and exporter. A Retivus-only round-trip does **not** prove Desktop compatibility.

## Fixture contract

The reproducible offline fixture contains a dedicated deck, four notes with multilingual Unicode/HTML fields, and five cards (including a distinct two-template bidirectional note type). It contains no personal review history or credentials. The CLI writes into an **existing absolute folder** and refuses to overwrite a pre-existing package.

```sh
cargo run --features anki-backend --bin desktop-interop-fixture -- generate /absolute/empty/test-directory
```

## Real Anki Desktop acceptance steps (still pending)

1. Open an isolated disposable profile in the **official Anki Desktop GUI**. Do not use an ordinary study profile.
2. Import `retivus-desktop-interop.apkg` using **File → Import**. Inspect the deck, four exact notes, five cards, Unicode fields, HTML bold markup and reverse-card note type.
3. Export that deck through **File → Export** to a new absolute `.apkg` path, **without scheduling/review history**. Do not rename the original Retivus export in place.
4. Independently check the Desktop-produced package through Retivus:

```sh
cargo run --features anki-backend --bin desktop-interop-fixture -- verify /absolute/path/from/anki-desktop.apkg
```

5. Record the official Anki Desktop version, OS, date, screenshot evidence and the exact CLI verification result. Only then consider M4.2 acceptance.

A CI self-check runs `generate` followed by `verify` to ensure the fixture tool itself is correct. That self-check is **not** a replacement for steps 1–5. M4.5 AnkiWeb sync is separately scoped and is not demonstrated by local Desktop import/export.
