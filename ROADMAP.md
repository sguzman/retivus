# Retivus — weighted implementation roadmap

**Scope freeze:** general-purpose Rust/egui spaced repetition with demonstrable Anki interoperability **and** independently versioned external data sources. [Product definition](docs/product.md). The [architecture gate](docs/architecture.md) decides how to reuse Anki's Rust backend without guessing. All weights represent **whole-project acceptance**, not arbitrary percentages of code written.

**Verified progress:** **53%** of 100% (previous verified checkpoint 51%, this continuation **+2 percentage points**). Native egui compilation, unit tests, and virtual X11 launch were verified. Full Linux desktop design QA remains scheduled for M5.6. Milestone weights deliberately distribute actual work across all eight milestones, not hide it in the last 10%.

**Status key:** ✅ verified complete · 🟡 active · ⏳ pending · 🚫 blocked. Each numbered line is one independently checkable work item; an item contributes its full specified weight only after its acceptance passes. No partial credit for drafts.

| Milestone | Weight | Verified | Status |
| --- | ---: | ---: | --- |
| 🧭 M0 Specification, project governance, initial foundation | 8% | 8% | ✅ |
| 🔬 M1 Anki Rust backend feasibility and licensing gate | 12% | 12% | ✅ |
| 🗃️ M2 Local collection engine and learner-safe core | 15% | 15% | ✅ |
| 🌍 M3 Generic external-source provider engine | 14% | 14% | ✅ |
| 🃏 M4 Anki ecosystem round-trip compatibility | 17% | 0% | ⏳ |
| 🎨 M5 Complete native egui study experience | 14% | 4% | 🟡 |
| 🧪 M6 Integration hardening, performance and correctness | 12% | 0% | ⏳ |
| 🚀 M7 Release, installation and handoff | 8% | 0% | ⏳ |

## Milestones, phases and objective criteria

### 🧭 M0: Specification, project governance, initial foundation (8% of project)

- ✅ **M0.1 🧭 Establish dual-purpose scope, exclusions and release acceptance** — **2%**. Acceptance: Product charter and stable definition of done are committed. Depends on: none. **verified complete.**
- ✅ **M0.2 🗺️ Publish weighted, task-level roadmap and update protocol** — **2%**. Acceptance: All milestone tasks include ownerless objective criteria and dependencies. Depends on: M0.1. **verified complete.**
- ✅ **M0.3 🦀 Compile native egui skeleton with CI** — **2%**. Acceptance: cargo fmt/clippy/test/build pass on Linux CI; visible window. Depends on: M0.1. **verified complete.**
- ✅ **M0.4 📜 Keep README, docs, CHANGELOG and contributor contract separated** — **2%**. Acceptance: All four artifact types maintained and navigation works. Depends on: M0.1. **verified complete.**

### 🔬 M1: Anki Rust backend feasibility and licensing gate (12% of project)

- ✅ **M1.1 🔎 Pin upstream Anki release/commit and dependency/toolchain inventory** — **2%**. Acceptance: Commit, transitive workspace deps and toolchain recorded. Depends on: M0.3. **verified complete.**
- ✅ **M1.2 ⚙️ Build actual pinned Anki Rust workspace in isolated CI** — **3%**. Acceptance: Repeatable Linux CI build passes. Depends on: M1.1. **verified complete.**
- ✅ **M1.3 🧩 Compile external client opening disposable Anki collection** — **3%**. Acceptance: Real read/write deck/note/card queue smoke without user collections. Depends on: M1.2. **verified complete.**
- ✅ **M1.4 ⚖️ Review AGPL licensing, notices, distribution and choose adapter** — **2%**. Acceptance: Document and implement legal compatible dependency boundary. Depends on: M1.3. **verified complete.**
- ✅ **M1.5 🧪 Confirm incremental upgrades/fallback and adapter tests** — **2%**. Acceptance: Pinned backend integration proven or justified alternative verified. Depends on: M1.4. **verified complete.**

### 🗃️ M2: Local collection engine and learner-safe core (15% of project)

- ✅ **M2.1 🗂️ Manage profiles, decks, notes, templates and multiple cards** — **4%**. Acceptance: Create/edit/delete/restore round-trips via selected backend. Depends on: M1.5. **verified complete.**
- ✅ **M2.2 ⏱️ Schedule learning/review with actual FSRS and due queues** — **4%**. Acceptance: Known scheduler fixtures and interrupted review restart pass. Depends on: M2.1. **verified complete.**
- ✅ **M2.3 💾 Persist review events/undo/backup/recovery atomically** — **4%**. Acceptance: Crash+rollback tests preserve previous state. Depends on: M2.1. **verified complete.**
- ✅ **M2.4 🔐 Profile isolation, path safety, migrations, export backups** — **3%**. Acceptance: Two-profile and corrupt-file tests pass. Depends on: M2.3. **verified complete.**

### 🌍 M3: Generic external-source provider engine (14% of project)

- ✅ **M3.1 🧱 Source-neutral versioned provider interface and errors** — **3%**. Acceptance: Two adapters use same interface and reject unsupported contracts. Depends on: M0.3. **verified complete.**
- ✅ **M3.2 📦 Stream Mexilex v0.3.0 source package with integrity/rights gates** — **4%**. Acceptance: Public archive, checksum and 124444-entry import smoke pass. Depends on: M3.1. **verified complete.**
- ✅ **M3.3 📝 Create note/card candidates with sense-disambiguating previews** — **3%**. Acceptance: Multiple homographs and senses never merge; selection tested. Depends on: M2.1,M3.2. **verified complete.**
- ✅ **M3.4 🔄 Snapshot diff, identity reconciliation and preserving history** — **3%**. Acceptance: Updated/missing/colliding input tests + undoable migration. Depends on: M2.3,M3.3. **verified complete.**
- ✅ **M3.5 🔌 Verify independent second provider, not Mexilex-specific** — **1%**. Acceptance: Distinct format imported and reviewed without modifying engine. Depends on: M3.1. **verified complete.**

### 🃏 M4: Anki ecosystem round-trip compatibility (17% of project)

- ⏳ **M4.1 📥 Import .apkg notes/decks/templates/tags/media safely** — **4%**. Acceptance: Official representative shared decks open and counts compare. Depends on: M1.5,M2.1. **pending.**
- ⏳ **M4.2 📤 Export .apkg with media and round-trip reimport** — **4%**. Acceptance: Anki desktop imports and reexports equivalent notes/cards. Depends on: M4.1. **pending.**
- ⏳ **M4.3 🧠 Preserve cloze, image/media, HTML card templates and CSS** — **4%**. Acceptance: Cloze and media rich fixture render parity passes. Depends on: M4.1. **pending.**
- ⏳ **M4.4 🔄 Safely open/migrate copy of real Anki collection** — **3%**. Acceptance: Copy-only migration, backup and version mismatch tests. Depends on: M2.3,M4.1. **pending.**
- ⏳ **M4.5 🌐 AnkiWeb sync compatibility or explicit separately agreed post-v1 classification** — **2%**. Acceptance: Authenticated test sync with conflicts, media and credential safety OR approved scope decision. Depends on: M4.4. **pending.**

### 🎨 M5: Complete native egui study experience (14% of project)

- ✅ **M5.1 🌗 System/light/dark visual themes and persistence** — **2%**. Acceptance: All modes render, OS follow, preference restart tests. Depends on: M0.3. **verified complete.**
- ✅ **M5.2 🔠 Typography, accessible density, sizing and keyboard controls** — **2%**. Acceptance: Krypton if installed, reliable fallback, layout and keyboard QA. Depends on: M5.1. **verified complete.**
- ⏳ **M5.3 🗂️ Deck browser, note editor, browse/search and import preview** — **3%**. Acceptance: Full keyboard-and-mouse workflow from import to cards. Depends on: M2.1,M3.3. **pending.**
- ⏳ **M5.4 🎯 Review UI, grading, undo, due counts and session transitions** — **3%**. Acceptance: Actual due queue and review state persisted across restart. Depends on: M2.2,M2.3. **pending.**
- ⏳ **M5.5 📊 Statistics, browse filters, source provenance and visible conflict UX** — **2%**. Acceptance: Metrics match backend and all provenance/conflict states visible. Depends on: M5.4. **pending.**
- ⏳ **M5.6 🖥️ Real GUI visual QA on Linux Wayland and window sizes** — **2%**. Acceptance: Screenshots/video evidence on actual app, not just compilation. Depends on: M5.1,M5.3,M5.4. **pending.**

### 🧪 M6: Integration hardening, performance and correctness (12% of project)

- ⏳ **M6.1 🧪 Repeatable fixtures/CI for backend, provider, scheduler and .apkg** — **3%**. Acceptance: End-to-end regression tests green. Depends on: M2,M3,M4. **pending.**
- ⏳ **M6.2 ⚡ Large dataset streaming, search and memory benchmarks** — **3%**. Acceptance: 124444-record Mexilex load without unbounded UI RAM. Depends on: M3.2,M5.3. **pending.**
- ⏳ **M6.3 🛡️ Corrupt/malicious archive, Unicode, media path and credential guards** — **3%**. Acceptance: Adversarial tests block zip slip, malformed data and leaks. Depends on: M4.1,M3.2. **pending.**
- ⏳ **M6.4 ♿ Keyboard, screen scaling, focus and accessibility regression** — **3%**. Acceptance: Real system QA checklist and automated smoke pass. Depends on: M5.6. **pending.**

### 🚀 M7: Release, installation and handoff (8% of project)

- ⏳ **M7.1 📄 Licenses, attribution, provider notices and documentation** — **2%**. Acceptance: Sources included; installer attribution verified. Depends on: M1.4,M4. **pending.**
- ⏳ **M7.2 📦 Reproducible Linux desktop release and install/update flow** — **3%**. Acceptance: Versioned distributable clean-machine install/uninstall tested. Depends on: M6. **pending.**
- ⏳ **M7.3 🧯 Upgrade/recovery test matrix and no known critical defects** — **2%**. Acceptance: Backups/migration/release gate CI pass. Depends on: M7.2. **pending.**
- ⏳ **M7.4 🏁 Publish release, tag and final acceptance ledger** — **1%**. Acceptance: Release assets independently downloaded and verified. Depends on: M7.1,M7.2,M7.3. **pending.**

## Prior verified checkpoint — 2026-11-01 (33%)

- ✅ **M2.1 🗂️ (+4 pp):** [Anki feature CI 41268857882](https://github.com/sguzman/retivus/actions/runs/41268857882) passed **20** feature-enabled tests, including an actual two-template Anki note yielding two cards, deck and note-type enumeration, note field modification, deletion, undo restoration, and a clean close/reopen preserving data. The alternate formatting-only build is still running but the tested source behavior is identical. This confirms the backend-level collection CRUD, not the still-pending egui editor or backup/recovery.


**Progress**: 23% → **33%** (**+10 percentage points** this session). M0: 8/8 (100%); M1: 12/12 (100%); M2: 4/15 (26.7%); M3: 7/14 (50%); M4: 0/17 (0%); M5: 2/14 (14.3%); M6: 0/12 (0%); M7: 0/8 (0%).

- ✅ **M1.5 🧪 (+2 pp):** [Feature CI 41267822664](https://github.com/sguzman/retivus/actions/runs/41267822664) passed **15** feature-enabled Rust tests, including Retivus-owned profile creation/reopen/isolation, schema 11 → 18 migration preserving notes, refusal of unknown schema 99 without deletion, and symlink path rejection. The backend stays pinned to Anki revision `136bd42bfbd356fc756b6d3e28cc77b02da11ea7`; no personal Anki profile was accessed. This verifies the scoped adapter upgrade/fallback gate, **not** full future Anki compatibility or production backup/recovery.
- ✅ **M3.2 📦 (+4 pp):** [Published source CI 41268497201](https://github.com/sguzman/retivus/actions/runs/41268497201) checked upstream Mexilex v0.3.0 SHA256SUMS, license/manifest and imported **124,444** entries and **155,586** senses into a separate SQLite catalog. [Rust CI 41268551513](https://github.com/sguzman/retivus/actions/runs/41268551513) passed catalog rollback, duplicate, provider-isolation and disk-reopen regression tests. The import is atomic per snapshot, and does **not** create cards or mutate user review history.
- ✅ **GUI smoke harness:** [Rust CI 41268276012](https://github.com/sguzman/retivus/actions/runs/41268276012) passed a real native window smoke with an X11 window manager and direct app-window screenshot. This only verifies launch and basic rendering; actual multi-size visual QA remains M5.6.
- ✅ **Previous M0/M1/M3/M5 work:** pinned Anki Rust build and direct Git dependency, AGPL provenance/attribution, System/Light/Dark theme persistence and OS following, source-neutral Mexilex/TSV adapters, bounded read-only preview, and separated README/docs/changelog remain verified by earlier runs documented in [technical architecture](docs/architecture.md), [providers](docs/providers.md), and [licensing](docs/licensing.md).
- ⏳ **Next:** M2.2 ⏱️ implement and verify actual Anki FSRS review/queue behavior across restart. M2.3 💾 adds undo/review-history crash safety and backups. M3.3 📝 builds lexical sense-to-card candidate previews now that M2.1 is verified. No new workstreams are introduced outside the fixed roadmap.

**Accounting rule:** only completed numbered tasks earn their fixed project weights. The provider import is a **source catalog**; this is not the later study-card generation, snapshot reconciliation or Anki package importer. Do not count GUI demo or documentation as any additional percentage.

## Verified continuation checkpoint — 2026-11-01 (37%)

**Progress:** **33% → 37% (+4 percentage points)**. M0: 8/8 (100%); M1: 12/12 (100%); M2: 4/15 (26.7%); M3: 11/14 (78.6%); M4: 0/17 (0%); M5: 2/14 (14.3%); M6: 0/12 (0%); M7: 0/8 (0%).

- ✅ **M3.3 📝 (+3 pp):** [Anki feature CI 41309537491](https://github.com/sguzman/retivus/actions/runs/41309537491) passed **27/27** library tests including independently selected Japanese-to-Spanish homographs, multiple distinct Mexilex senses, persistent Anki-owned notes, preserved source identity tags and review history on reopen. [Rust CI 41309537553](https://github.com/sguzman/retivus/actions/runs/41309537553) passed the native read-only explicit-sense preview selection regression. This validates source-to-note candidate selection, **not** completed deck management GUI, automatic snapshot migration, media or package import/export.
- ✅ **M3.5 🔌 (+1 pp):** The same green [Rust CI 41309537553](https://github.com/sguzman/retivus/actions/runs/41309537553) passed the independent `import-generic` Japanese-to-Spanish TSV CLI import and disk-reopen preview. Feature CI 41309537491 then imported its three records through the unchanged source-neutral catalog, selected its homographs, created Anki notes and persisted an actual graded review without altering Mexilex logic.
- ✅ **M2.2 ⏱️ verified (+4 pp in next checkpoint):** [Feature CI 41310578538](https://github.com/sguzman/retivus/actions/runs/41310578538) passed all 30 tests including known four-rating scheduler transitions, revlog consistency, review undo and interrupted restart.
- 🟡 **M3.4 🔄 pending, 0/3 pp:** The newly added reversible source-tag parser is a foundation only; no snapshot diff, conflict mediation, history-preserving migration or undoable reconciliation was claimed complete.
- ⏳ **Next:** Verify source-tag parser CI, add deterministic scheduler grade/state fixtures to finish M2.2, and build learner-safe backup/recovery for M2.3. Current GUI remains read-only, and complete multi-size visual QA is still pending.

## Verified continuation checkpoint — 2026-11-01 (41%)

**Progress:** **37% → 41% (+4 percentage points)**. M0: 8/8 (100%); M1: 12/12 (100%); M2: 8/15 (53.3%); M3: 11/14 (78.6%); M4: 0/17 (0%); M5: 2/14 (14.3%); M6: 0/12 (0%); M7: 0/8 (0%).

- ✅ **M2.2 ⏱️ (+4 pp):** [Anki feature CI 41310578538](https://github.com/sguzman/retivus/actions/runs/41310578538) passed **30 tests**: all Again/Hard/Good/Easy ratings, persisted upstream FSRS scheduling state and revlog, queue restart, and explicit undo of a review. [Rust foundation 41310578551](https://github.com/sguzman/retivus/actions/runs/41310578551) passed format, clippy, provider and native smoke.
- 🟡 **M2.3 💾 0/4 pp, implementation in verification:** Non-clobber SQLite snapshot and restore-as-new-profile paths under development; no backup/recovery credit until destructive, corrupt-file and rollback tests actually pass. Media packaging and migration remain separate.

## Verified continuation checkpoint — 2026-11-01 (45%)

**Progress:** **41% → 45% (+4 percentage points)**. M0: 8/8 (100%); M1: 12/12 (100%); M2: 12/15 (80%); M3: 11/14 (78.6%); M4: 0/17 (0%); M5: 2/14 (14.3%); M6: 0/12 (0%); M7: 0/8 (0%).

- ✅ **M2.3 💾 (+4 pp):** [Anki feature CI 41312321565](https://github.com/sguzman/retivus/actions/runs/41312321565) passed **32** library tests including a live SQLite snapshot preserving one FSRS review while newer original-profile notes remain untouched, restore into a separate managed profile, refusal of corrupt/symlinked input and existing backup/profile targets, review-event undo and restart. [Rust foundation 41312321541](https://github.com/sguzman/retivus/actions/runs/41312321541) passed native build, Clippy and GUI smoke. Anki's registered `unicase` collation is used for staged integrity verification, rather than invalid checking with a raw connection. A full forced power-failure fixture remains M6 hardening; the backup is the collection DB only, not media.
- 🟡 **M2.4 🔐 0/3 pp:** Existing isolation/schema/symlink/backup tests are green. An explicit corrupted original-collection refusal regression is newly authored and requires feature-CI acceptance before this milestone is counted.
- ⏳ **Next:** Finish M2.4 acceptance, then implement undoable provider snapshot reconciliation (M3.4). Native GUI deck browser, review UX and Anki package round trips remain separately scoped.

## Verified continuation checkpoint — 2026-11-01 (48%)

**Progress:** **45% → 48% (+3 percentage points)**. M0: 8/8 (100%); M1: 12/12 (100%); M2: 15/15 (100%); M3: 11/14 (78.6%); M4: 0/17 (0%); M5: 2/14 (14.3%); M6: 0/12 (0%); M7: 0/8 (0%).

- ✅ **M2.4 🔐 (+3 pp):** [Anki feature CI 41313034807](https://github.com/sguzman/retivus/actions/runs/41313034807) passed **33/33** tests: isolated profiles, path/symlink rejection, supported schema-11 migration preserving notes, unknown future-version refusal, corrupt original-database refusal without replacing its contents, and the new backup/recovery smoke. All **15/15 M2** points now verified.
- 🟡 **M3.4 🔄 0/3 pp:** Bounded source snapshot differences and explicit one-to-one reconciliations are implemented but feature/Anki note migration tests remain unverified. Neither provider words nor source metadata may silently overwrite learner field edits or review history.
- ⏳ **Next:** Verify M3.4 end-to-end and handle conflicts before credit; then pursue scoped Anki package round trips and native editor/review controls.

## Verified continuation checkpoint — 2026-11-01 (51%)

**Progress:** **48% → 51% (+3 percentage points)**. M0: 8/8 (100%); M1: 12/12 (100%); M2: 15/15 (100%); M3: 14/14 (100%); M4: 0/17 (0%); M5: 2/14 (14.3%); M6: 0/12 (0%); M7: 0/8 (0%).

- ✅ **M3.4 🔄 (+3 pp):** [Rust foundation CI 41313849974](https://github.com/sguzman/retivus/actions/runs/41313849974) passed **20 library tests** and independent provider/native smoke, including bounded diff of changed/removed/new senses, explicit homograph collision behavior, one-to-one conflict rejection, immutable original identities, persisted source mappings and reversible undo after SQLite reopen. [Anki feature CI 41313808023](https://github.com/sguzman/retivus/actions/runs/41313808023) passed **35/35** including application of a catalog-approved source migration to the same Anki note while retaining the same card review log, rejecting manually edited fields, undo, and persistent reopen. Thus **M3 is 14/14**. The engine does **not** auto-reconcile ambiguous sources or expose finished GUI conflict resolution (M5.5).
- ⏳ **Next:** M4.1/M4.2 Anki `.apkg` imports and exports with real media/card/template round trips, followed by native deck/notes/review UI M5 and independent release hardening M6/M7. Do not equate pinned upstream feature gate with AnkiWeb sync compatibility.

## Continuation verification — 2026-11-01 (51%, +0 pp)

- 🟡 **M4.1 📥 (0/4 pp):** [Anki feature CI 41320662558](https://github.com/sguzman/retivus/actions/runs/41320662558) passed the isolated import of two exact-commit, Git-blob-verified third-party `.apkg` fixtures (official AnkiDroid test package and public MIT sample deck), malformed package refusal, and unchanged note/card counts after export/reimport. Official AnkiWeb shared-deck coverage remains unverified against the exact milestone acceptance criterion.
- 🟡 **M4.2 📤 (0/4 pp):** Pinned Anki Rust backend passes media-inclusive package export/import and note/card count parity; separate Anki Desktop round-trip acceptance remains unverified.
- 🟡 **M4.3 🧠 (0/4 pp):** [Anki feature CI 41316665117](https://github.com/sguzman/retivus/actions/runs/41316665117) passed cloze cards, HTML/CSS rendering and valid SVG media equality before and after package reimport. The M4.1 prerequisite and external rich-deck QA remain open.
- 🟡 **M4.4 🔄 (0/3 pp):** [Anki feature CI 41320662558](https://github.com/sguzman/retivus/actions/runs/41320662558) passed the official upstream `.anki2` file migration as a non-destructive copy, with schema verification and preservation of source bytes; prior [CI 41319176926](https://github.com/sguzman/retivus/actions/runs/41319176926) checked WAL refusal, symlinks, existing destination, malformed source and schema-11→18 migration. The M4.1 prerequisite remains open. Media-folder migration is intentionally not implicit.
- 🟡 **M5.2 🔠 (0/2 pp):** [Rust foundation CI 41320966812](https://github.com/sguzman/retivus/actions/runs/41320966812) passed persisted 80–150% UI scale, compact/comfortable density, responsive toolbar and source input layouts, valid Krypton fallback, Clippy and actual native X11 smoke. Explicit keyboard, window-size and Wayland QA remain pending.
- ⏳ **Next acceptance:** Check officially shared AnkiWeb deck fixtures with grounded provenance for M4.1 and independent Anki Desktop import/export for M4.2; then complete the dependent M4 tasks. Native deck editor/review and full accessibility remain M5 work. Do not convert implementation-only tests into weighted completion before acceptance.

## Verified continuation checkpoint — 2026-11-01 (53%)

**Progress:** **51% → 53% (+2 percentage points)**. M0: 8/8 (100%); M1: 12/12 (100%); M2: 15/15 (100%); M3: 14/14 (100%); M4: 0/17 (0%); M5: 4/14 (28.6%); M6: 0/12 (0%); M7: 0/8 (0%).

- ✅ **M5.2 🔠 (+2 pp):** [Rust foundation CI 41324079917](https://github.com/sguzman/retivus/actions/runs/41324079917) passed formatting, strict Clippy, real native GUI smoke, keyboard focus navigation and 680×560/1100×760 X11 screenshot captures ([run artifact](https://github.com/sguzman/retivus/actions/runs/41324079917)). Screenshots inspected: controls and content remain legible at both sizes; Return activated the keyboard-focused Appearance menu. Existing local Krypton Nerd Font detection rejects unsafe files and falls back without bundling fonts. System/Light/Dark, compact/comfortable density and persisted 80–150% scaling pass code-level tests. Multi-platform Wayland visual QA and full accessibility remain separately weighted M5.6/M6.4.
- 🟡 **M5.3/M5.4 (0 pp new):** Feature-gated, explicitly owned learner profiles with Anki-backed notes/decks, browse/search, import/export, due counts and answer reveal/grading now have a native UI. Backend preview is bound to the exact card expected by grading. Feature-enabled binary compile and end-to-end mouse/keyboard tests are pending CI acceptance; rich HTML rendering is displayed inertly instead of executing embedded third-party code.
- ⏳ **Next:** Verify feature-enabled native learner UI; continue unresolved M4 Anki Desktop interoperability and M5.3/M5.4 integration.
