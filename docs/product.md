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
- Local-only for v1, by the owner's explicit 2026-11-08 scope decision. The Anki Rust library serves local collections/scheduling and does **not** require an AnkiWeb account. Remote sync remains experimental, unverified, and outside v1; importing or studying a deck never silently triggers synchronization.
- Make import failures, source licenses, deck identity, and update conflicts visible and actionable rather than silently guessing.
- Respect Anki, provider and media licensing. Never copy proprietary trademarks/assets as a Retivus brand.

## Release boundary

A 100% first stable release requires the complete selected roadmap, including real Anki package interoperability and one production-ready external-provider import and upgrade flow, persistent review scheduling, tested GUI, packaging, and an appropriate license. Automatic AnkiWeb sync is only accepted if upstream compatibility and account safety tests pass. The owner explicitly chose the documented **post-v1 classification** on 2026-11-08; therefore this is **not a v1 dependency** and is not represented as validated. Real-workstation keyboard, focus, scaling, contrast/reduced-motion and review/restart usability QA remains mandatory, as do licensing and release acceptance. Per the owner's 2026-11-09 clarification, installing or using Orca is **not** a prerequisite for user QA or v1 release. Independent screen-reader evaluation may proceed separately if explicitly requested and must not be represented as verified without evidence.

## Exclusions

Not an in-app dictionary editor, scraping engine, universal ontology, collaborative SaaS, Anki add-on interpreter, or a forked copy of Mexilex. Do not change the objective merely to fill a progress meter.
