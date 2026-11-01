# Retivus engineering agent contract

Authoritative scope: [docs/product.md](docs/product.md). Progress source: [ROADMAP.md](ROADMAP.md). Decisions: [docs/architecture.md](docs/architecture.md). Release history: [CHANGELOG.md](CHANGELOG.md).

## Execution
- Continue autonomously toward the **fixed 100% release target**, working directly on `main` unless instructed otherwise. Inspect the smallest relevant files, CI failures and last verified checkpoint; conserve chat context.
- Implement and test, then commit. Never claim a GUI was visually verified without real rendering and user-visible QA. Never call a package/Anki sync compatible without real round-trip validation.
- Every task ID gets one emoji, a weight, dependencies, verifiable acceptance criteria and an honest state. Verified points are earned *only* after the criterion is satisfied. Maintain milestone weights without leaving the majority of the work for the final 10%.
- Each round report **overall progress bar, previous %, current %, percentage-point delta, current milestone %, only newly touched tasks, actual tests, commit and blockers**. If delta is 0, name the precise blocker and the next mitigation; do not invent work to force progress.
- Avoid giant raw logs, whole-repo repeat discovery, unnecessary comments and extra summaries. Prefer small command output, targeted CI steps, source-local patches and one consolidated commit for related changes.
- The README describes the product and how to start, never logs daily work. `docs/` contains detailed design and `CHANGELOG.md` records releases/meaningful change groups.

## Boundaries
- 🦀 Rust + egui; no Python/Qt/npm runtime dependency for Retivus.
- 🌗 System (default), Light, Dark GUI appearance, keyboard UX, compact typography and Krypton Nerd Font where locally available with safe fallback. Use the canonical [OKLCH brand palette](docs/branding.md) via `src/brand.rs`, not invented ad hoc accent hues.
- 🔌 Dual purpose: Anki ecosystem and multiple external data providers; Mexilex is first, not hardwired.
- 🔒 Separate external source records from private cards/history; never upload histories or silently auto-migrate ambiguities.
- ⚖️ Anki upstream `rslib` is AGPL; review licensing before linking or distributing; original project license remains as initialized until a documented decision.
- 🧭 No arbitrary new workstreams after scope locked, no unrelated features, no names ending in the prohibited suffix. Never erase an existing profile or user's collection.
- ▶️ “continue” means continue only if legitimate roadmap work remains. Stop at verified completion.
