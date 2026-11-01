# Product scope and acceptance

## Dual purpose

1. **Anki ecosystem:** Import/export shared Anki packages, preserve deck/note/card semantics and scheduled reviews, and target safe interoperability with Anki collections and AnkiWeb when explicitly verified. Anki add-on API parity is **not** assumed; Python/Qt add-ons will not magically run in egui.
2. **Independent data:** Consume externally produced machine-readable vocabularies (starting with Mexilex), let users select meanings and create cards, and reconcile source updates without mutating personal progress. Provider support must remain generic and independently testable.

## Product principles

- The **learning** entity is a locally owned reviewable card. The **source** entity is a separately versioned reference record. A note may create multiple cards; one word may have several senses.
- All operations must preserve user-created content and progress across imports, upgrades, errors and application restarts.
- No arbitrary word ceiling. Import should stream and avoid loading entire catalogs in memory.
- No Python runtime, Qt, npm, or hosted backend required to run the initial desktop build.
- Linux (Wayland/Hyprland) is the first verified desktop target. Other targets require their own release criteria.
- GUI has a System (default), Light and Dark selector. System responds to OS appearance. Typography favors **Krypton Nerd Font if locally installed**, with bundled-free fallback to standard egui fonts; never distribute third-party fonts without permission. Legibility, compactness, no overrun, keyboard-first input, and focus control matter.
- Offline-first. Sync requires an explicit credential and interoperability/security phase and is never promised by merely importing a deck.
- Make import failures, source licenses, deck identity, and update conflicts visible and actionable rather than silently guessing.
- Respect Anki, provider and media licensing. Never copy proprietary trademarks/assets as a Retivus brand.

## Release boundary

A 100% first stable release requires the complete selected roadmap, including real Anki package interoperability and one production-ready external-provider import and upgrade flow, persistent review scheduling, tested GUI, packaging, and an appropriate license. Automatic AnkiWeb sync is only 100% when upstream compatibility and account safety tests pass; documented fallback is to classify as a separately approved post-v1 scope rather than silently claiming parity.

## Exclusions

Not an in-app dictionary editor, scraping engine, universal ontology, collaborative SaaS, Anki add-on interpreter, or a forked copy of Mexilex. Do not change the objective merely to fill a progress meter.
