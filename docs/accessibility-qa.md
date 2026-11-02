# Native accessibility QA — development acceptance checklist

This document scopes **M6.4** against Retivus's actual Rust/egui desktop UI. Automation is necessary but does not replace a screen-reader or assistive-technology evaluation. Keep unverified items pending.

## Native platform accessibility bridge

Retivus disables eframe default features to control its native build. Its explicit `accesskit` feature is now enabled in `Cargo.toml` alongside X11 and Wayland so the application can expose its standard egui widget tree to supported Linux AT-SPI assistive technology. This is a **build-time prerequisite**, not evidence that Orca actually announced content correctly. A real accessibility bus/screen-reader test and dynamic status announcements still need verification; do not mark M6.4 complete just because the feature compiles.

## Automated coverage

| Contract | Test/evidence | Status |
| --- | --- | --- |
| AccessKit platform accessibility implementation compiled into release profile | Explicit `eframe/accesskit` Cargo feature, plus native build CI | **Pending CI; no assistive-technology QA claim** |
| Keyboard focus follows Tab and Shift+Tab | Native egui_kittest source-file focus and reversal test in src/main.rs | Verified in feature CI 41347374839 |
| Native pointer controls actually complete a review | Source validation, precise sense selection, card creation, grade and persistence via egui_kittest | Verified in feature CI 41347374839 |
| Note edit/search controls respond to keyboard/mouse | Native editor and search end-to-end regression in src/study_ui.rs | Verified in feature CI 41347374839 |
| System, Light and Dark persist correctly | App preference tests in src/main.rs | Verified in foundation CI |
| Scale, density, window resizing, labelled input focus and Enter-activated validation | `accessibility_focus_survives_scaling_density_theme_and_resizing` in `src/main.rs` | Regression strengthened; **pending new CI** |
| Real compositor renders wide and narrow windows | Retained native Sway Wayland 1080×800 and 720×650 screenshot artifact, reviewed for initial-window clipping and missing glyphs | Verified in Anki CI 41343238570 |

## Real-system visual and assistive-technology checks

- [x] X11 and Sway Wayland native windows launch in CI; initial-window screenshots inspected at multiple sizes
- [x] Real keyboard/pointer test navigates source entry selection, grading and persistence
- [ ] Inspect **populated** browse, source conflict and reviewer states at both scales in a physical Wayland session
- [ ] Verify screen-reader announcements of form labels, deck choices, status updates and due-card transitions with an actual screen reader
- [ ] Validate high-contrast mode and any reduced-motion platform preference against user testing

**Scope:** No automatic success or milestone credit for unchecked manual items. Check only observed evidence. Keyboard Enter activation after each theme, scale, density and resize state is now an explicit automated gate, but must pass CI before being credited. Screen-reader announcements, high-contrast/reduced-motion behavior and populated real-system windows still need independent acceptance; broad assistive-technology support is not claimed.
