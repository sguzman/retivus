# Retivus — weighted implementation roadmap

**Scope freeze:** general-purpose Rust/egui spaced repetition with demonstrable Anki interoperability **and** independently versioned external data sources. [Product definition](docs/product.md). The [architecture gate](docs/architecture.md) decides how to reuse Anki's Rust backend without guessing. All weights represent **whole-project acceptance**, not arbitrary percentages of code written.

**Verified progress:** **23%** of 100% (previous verified 13%, latest development session **+10 percentage points**). Native egui compilation, unit tests, and virtual X11 launch were verified. Full Linux desktop design QA remains scheduled for M5.6. Milestone weights deliberately distribute actual work across all eight milestones, not hide it in the last 10%.

**Status key:** ✅ verified complete · 🟡 active · ⏳ pending · 🚫 blocked. Each numbered line is one independently checkable work item; an item contributes its full specified weight only after its acceptance passes. No partial credit for drafts.

| Milestone | Weight | Verified | Status |
| --- | ---: | ---: | --- |
| 🧭 M0 Specification, project governance, initial foundation | 8% | 8% | ✅ |
| 🔬 M1 Anki Rust backend feasibility and licensing gate | 12% | 10% | 🟡 |
| 🗃️ M2 Local collection engine and learner-safe core | 15% | 0% | ⏳ |
| 🌍 M3 Generic external-source provider engine | 14% | 3% | 🟡 |
| 🃏 M4 Anki ecosystem round-trip compatibility | 17% | 0% | ⏳ |
| 🎨 M5 Complete native egui study experience | 14% | 2% | 🟡 |
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
- 🟡 **M1.5 🧪 Confirm incremental upgrades/fallback and adapter tests** — **2%**. Acceptance: Pinned backend integration proven or justified alternative verified. Depends on: M1.4. **active.**

### 🗃️ M2: Local collection engine and learner-safe core (15% of project)

- ⏳ **M2.1 🗂️ Manage profiles, decks, notes, templates and multiple cards** — **4%**. Acceptance: Create/edit/delete/restore round-trips via selected backend. Depends on: M1.5. **pending.**
- ⏳ **M2.2 ⏱️ Schedule learning/review with actual FSRS and due queues** — **4%**. Acceptance: Known scheduler fixtures and interrupted review restart pass. Depends on: M2.1. **pending.**
- ⏳ **M2.3 💾 Persist review events/undo/backup/recovery atomically** — **4%**. Acceptance: Crash+rollback tests preserve previous state. Depends on: M2.1. **pending.**
- ⏳ **M2.4 🔐 Profile isolation, path safety, migrations, export backups** — **3%**. Acceptance: Two-profile and corrupt-file tests pass. Depends on: M2.3. **pending.**

### 🌍 M3: Generic external-source provider engine (14% of project)

- ✅ **M3.1 🧱 Source-neutral versioned provider interface and errors** — **3%**. Acceptance: Two adapters use same interface and reject unsupported contracts. Depends on: M0.3. **verified complete.**
- ⏳ **M3.2 📦 Stream Mexilex v0.3.0 source package with integrity/rights gates** — **4%**. Acceptance: Public archive, checksum and 124444-entry import smoke pass. Depends on: M3.1. **pending.**
- ⏳ **M3.3 📝 Create note/card candidates with sense-disambiguating previews** — **3%**. Acceptance: Multiple homographs and senses never merge; selection tested. Depends on: M2.1,M3.2. **pending.**
- ⏳ **M3.4 🔄 Snapshot diff, identity reconciliation and preserving history** — **3%**. Acceptance: Updated/missing/colliding input tests + undoable migration. Depends on: M2.3,M3.3. **pending.**
- ⏳ **M3.5 🔌 Verify independent second provider, not Mexilex-specific** — **1%**. Acceptance: Distinct format imported and reviewed without modifying engine. Depends on: M3.1. **pending.**

### 🃏 M4: Anki ecosystem round-trip compatibility (17% of project)

- ⏳ **M4.1 📥 Import .apkg notes/decks/templates/tags/media safely** — **4%**. Acceptance: Official representative shared decks open and counts compare. Depends on: M1.5,M2.1. **pending.**
- ⏳ **M4.2 📤 Export .apkg with media and round-trip reimport** — **4%**. Acceptance: Anki desktop imports and reexports equivalent notes/cards. Depends on: M4.1. **pending.**
- ⏳ **M4.3 🧠 Preserve cloze, image/media, HTML card templates and CSS** — **4%**. Acceptance: Cloze and media rich fixture render parity passes. Depends on: M4.1. **pending.**
- ⏳ **M4.4 🔄 Safely open/migrate copy of real Anki collection** — **3%**. Acceptance: Copy-only migration, backup and version mismatch tests. Depends on: M2.3,M4.1. **pending.**
- ⏳ **M4.5 🌐 AnkiWeb sync compatibility or explicit separately agreed post-v1 classification** — **2%**. Acceptance: Authenticated test sync with conflicts, media and credential safety OR approved scope decision. Depends on: M4.4. **pending.**

### 🎨 M5: Complete native egui study experience (14% of project)

- ✅ **M5.1 🌗 System/light/dark visual themes and persistence** — **2%**. Acceptance: All modes render, OS follow, preference restart tests. Depends on: M0.3. **verified complete.**
- ⏳ **M5.2 🔠 Typography, accessible density, sizing and keyboard controls** — **2%**. Acceptance: Krypton if installed, reliable fallback, layout and keyboard QA. Depends on: M5.1. **pending.**
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

## ✅ Verified integration checkpoint — 2026-11-01

- **M1.3 🧩 (3 points):** [Anki CI run 41266034216](https://github.com/sguzman/retivus/actions/runs/41266034216) compiled and executed the independent Retivus client using pinned `rslib` Rust APIs. It created an isolated in-memory Anki collection, deck, Basic note, card and review queue; no existing profile was accessed. Direct Git dependency packaging is being tested separately; this success is for the isolated path-based adapter.
- **M3.1 🧱 (3 points):** [Rust CI run 41266324012](https://github.com/sguzman/retivus/actions/runs/41266324012) passed the object-safe `SourceAdapter` contract for Mexilex JSONL and distinct generic TSV, explicit language/licensing metadata, duplicate/malformed rejection and streaming record visits.
- **M5.1 🌗 (2 points):** the same successful CI passed preference save/restart tests, all three egui theme-rendering paths, and OS light/dark input changes in System mode, in addition to the earlier virtual X11 application launch smoke. Full desktop visual styling and window-size QA remain M5.6.
- **M1.4 ⚖️ (verified, +2 points):** the optional pinned Cargo Git backend is now integrated with the Retivus core, CI-confirmed, AGPL-3.0-or-later licensed, and accompanied by an [Anki provenance notice](LICENSES/ANKI-ATTRIBUTION.md). Final release source packaging and complete upstream dependency notices remain separate M7 acceptance criteria.
- **M3.2 📦 (pending, uncredited):** public Mexilex release checksums and a 124,444-entry streaming validation passed in [run 41262927205](https://github.com/sguzman/retivus/actions/runs/41262927205). A durable transactional installation with rollback has not been built, so it is **not** counted as a completed import.
- **Current next gate:** test pinned Cargo Git dependency to avoid a manually maintained Anki checkout; then choose and implement the legally compliant shipping adapter and safe profile persistence.

**Session progress:** 13% → **23%** (**+10 percentage points**). M1 = 10/12 (83.3%); M3 = 3/14 (21.4%); M5 = 2/14 (14.3%). Only completed acceptance items earn weight. Do not count partial UI wiring or backend research as extra tasks.

## ✅ Direct Anki backend acceptance — 2026-11-01

- **M1.4 ⚖️ (+2 points):** [Anki feature CI run 41266965280](https://github.com/sguzman/retivus/actions/runs/41266965280) passed the production-library feature-gated adapter tests after pinned upstream path and Git smoke checks. `src/anki_backend.rs` opens disposable in-memory Anki collections, creates decks and Basic notes, and queries due/next card APIs. The feature uses the Git-pinned upstream `anki` crate with Tokio feature unification; no Anki profile is opened automatically.
- Default builds remain independent of the AGPL dependency compilation: [Rust CI run 41266965361](https://github.com/sguzman/retivus/actions/runs/41266965361) passed without enabling `anki-backend`. The project itself is AGPL-3.0-or-later, original MIT revisions remain historically licensed, and [Anki source attribution](LICENSES/ANKI-ATTRIBUTION.md) is committed.
- **M1.5 🧪 still pending:** compatibility upgrading from the pinned revision, fallback/upgrade behavior, and real adapter maintenance tests have not been verified. Persistent user collections, review grading and AnkiWeb synchronization remain unimplemented.

## ⚖️ License decision (unweighted)

The project adopted **AGPL-3.0-or-later**, in anticipation of combining Retivus with Anki's upstream AGPL Rust backend. The original MIT grant is preserved in `LICENSES/MIT-original.txt` and [licensing history](docs/licensing.md). **M1.4 remains pending** until the backend integration is verified and its actual redistribution/notice obligations are implemented; the license declaration alone earns no points.

## 🎨 Brand identity amendment (unweighted)

Retivus now owns a distinct OKLCH project color and shared theme tokens, documented in [docs/branding.md](docs/branding.md) and applied in the native egui shell. This is part of the existing M5.1 theme task, **not** a new scope item or extra points: M5.1 is verified through persistence, rendering, and system-theme unit tests; final multi-window visual QA remains M5.6.

## Verified evidence and next gate (2026-11-01)

- ✅ M0.1, M0.2, M0.4: fixed product scope, weighted roadmap, README/docs/changelog/agent separation.
- ✅ M0.3: Rust formatting, Clippy, tests, compiled egui app and actual X11 virtual-window smoke, screenshot artifact. [Foundation run 41262584259](https://github.com/sguzman/retivus/actions/runs/41262584259). Real full GUI design QA still M5.6.
- ✅ M1.1: pinned Anki source SHA `136bd42bfbd356fc756b6d3e28cc77b02da11ea7`, Rust 1.99.0, 12-member workspace, generated-protobuf/toolchain/native deps and AGPL boundary [inventoried](docs/anki-backend-spike.md).
- ✅ M1.2: [Anki build run 41262898809](https://github.com/sguzman/retivus/actions/runs/41262898809) compiled real pinned `rslib` under Ubuntu CI **after** installing `protoc`, two translation submodules and an explicit isolated `tokio/io-util` feature adjustment to the temporary upstream workspace. This does **not** claim that unmodified upstream compiles standalone or that Retivus embeds it.
- 🟡 M1.3: independent disposable external client added to `spikes/anki-collection/`; **pending verification** of in-memory collection/deck/note/card/queue, no earned points yet.
- 🟡 M3.2: [real published Mexilex v0.3.0 consumer run 41262927205](https://github.com/sguzman/retivus/actions/runs/41262927205) verified public SHA256SUMS, license, manifest/language contract and streamed all **124,444** study records with Rust. **No M3 credit yet:** durable transactional import into learner profile and generic provider engine are not implemented.
- Next: verify external Anki client; decide adapter/licensing only after API spike; build source-neutral provider integration and safe persistent data and reviews.

**Progress accounting:** pre-init 0% → first-round **13%** (+13 percentage points). Previously verified 10% → **13%** (+3 for M1.2). M0 is 8/8 = 100%; M1 is 5/12 = 41.7%. Nothing in unfinished M3/M4/M5 counts as complete.
