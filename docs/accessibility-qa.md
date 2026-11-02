# Native accessibility QA — development acceptance checklist

This document scopes **M6.4** against Retivus's actual Rust/egui desktop UI. Automation is necessary but does not replace a screen-reader or assistive-technology evaluation. Keep unverified items pending.

## Native platform accessibility bridge

Retivus disables eframe default features to control its native build. Its explicit `accesskit` feature is now enabled in `Cargo.toml` alongside X11 and Wayland so the application can expose its standard egui widget tree to supported Linux AT-SPI assistive technology. This is a **build-time prerequisite**, not evidence that Orca actually announced content correctly. A real accessibility bus/screen-reader test and dynamic status announcements still need verification; do not mark M6.4 complete just because the feature compiles.

## Automated coverage

| Contract | Test/evidence | Status |
| --- | --- | --- |
| AccessKit platform accessibility implementation compiled into native app | Explicit `eframe/accesskit`, [foundation CI 41370243289](https://github.com/sguzman/retivus/actions/runs/41370243289) | **Verified build, not screen-reader usability** |
| Test-only Orca speech-debug and keyboard focus | [Orca CI 41371580320](https://github.com/sguzman/retivus/actions/runs/41371580320) confirmed spoken `File` and `Validate source`; new regression also requires Appearance/Density/Format labels | **Core labels verified; enhanced combo-label CI pending** |
| Real native Linux AT-SPI widget registration | [AT-SPI CI 41371103522](https://github.com/sguzman/retivus/actions/runs/41371103522) used an external client to discover `File` and `Validate source` among 23 accessible objects | **Verified native registration** |
| Keyboard focus follows Tab and Shift+Tab | Native egui_kittest source-file focus and reversal test in src/main.rs | Verified in feature CI 41347374839 |
| Native pointer controls actually complete a review | Source validation, precise sense selection, card creation, grade and persistence via egui_kittest | Verified in feature CI 41347374839 |
| Note edit/search controls respond to keyboard/mouse | Native editor and search end-to-end regression in src/study_ui.rs | Verified in feature CI 41347374839 |
| System, Light and Dark persist correctly | App preference tests in src/main.rs | Verified in foundation CI |
| Scale, density, window resizing, labelled input focus and Enter-activated validation | `accessibility_focus_survives_scaling_density_theme_and_resizing` in `src/main.rs` | **Verified foundation CI 41371103471** |
| Real compositor renders wide and narrow windows | Retained native Sway Wayland 1080×800 and 720×650 screenshot artifact, reviewed for initial-window clipping and missing glyphs | Verified in Anki CI 41343238570 |

## Real-system visual and assistive-technology checks

- [x] X11 and Sway Wayland native windows launch in CI; initial-window screenshots inspected at multiple sizes
- [x] Real keyboard/pointer test navigates source entry selection, grading and persistence
- [ ] Inspect **populated** browse, source conflict and reviewer states at both scales in a physical Wayland session
- [ ] Verify screen-reader announcements of form labels, deck choices, status updates and due-card transitions with an actual screen reader
- [ ] Validate high-contrast mode and any reduced-motion platform preference against user testing

**Scope:** Native AT-SPI registration and keyboard/scale/theme focus regression have passed CI. **M6.4 remains pending (0/3 points)** because actual screen-reader announcements, populated real Wayland states, high contrast and reduced motion have not been independently accepted. AT-SPI tree visibility alone is not proof of usable screen-reader navigation or announcements.
