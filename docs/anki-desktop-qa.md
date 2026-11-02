# Anki Desktop GUI round-trip QA — M4.2

**Status:** **accepted for M4.2**, via [official Desktop GUI CI 41368266062](https://github.com/sguzman/retivus/actions/runs/41368266062) and [inspected evidence artifact](https://github.com/sguzman/retivus/actions/runs/41368266062/artifacts/13108466264). This workflow uses official Anki Desktop's real GUI importer/exporter; a Retivus-only round-trip would not prove compatibility.

## Fixture contract

The reproducible offline fixture contains a dedicated deck, four notes with multilingual Unicode/HTML fields, and five cards (including a distinct two-template bidirectional note type). It contains no personal review history or credentials. The CLI writes into an **existing absolute folder** and refuses to overwrite a pre-existing package.

```sh
cargo run --features anki-backend --bin desktop-interop-fixture -- generate /absolute/empty/test-directory
```

## Real Anki Desktop acceptance steps (completed in isolated CI)

1. Open an isolated disposable profile in the **official Anki Desktop GUI**. Do not use an ordinary study profile.
2. Import `retivus-desktop-interop.apkg` using **File → Import**. Inspect the deck, four exact notes, five cards, Unicode fields, HTML bold markup and reverse-card note type.
3. Export that deck through **File → Export** to a new absolute `.apkg` path, **without scheduling/review history**. Do not rename the original Retivus export in place.
4. Independently check the Desktop-produced package through Retivus:

```sh
cargo run --features anki-backend --bin desktop-interop-fixture -- verify /absolute/path/from/anki-desktop.apkg
```

5. Record official Anki Desktop 26.09.3, Linux x86_64, 2026-11-02 UTC, [GUI screenshots](https://github.com/sguzman/retivus/actions/runs/41368266062/artifacts/13108466264) and CI output: `Verified Desktop-exported fixture: 4 notes, 5 cards, preserved fields/deck/template`. The automated GUI sequence passed; the exported archive was independently reimported and verified.

A separate CI self-check still runs `generate` followed by `verify` to ensure the fixture tool itself is correct; the actual Desktop import/reexport test above provides independent acceptance. The fixture has no private review history, so the GUI test does not independently demonstrate selective scheduling-history export. M4.5 AnkiWeb sync is separately scoped and is not demonstrated by local Desktop import/export.
