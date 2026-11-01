# Native accessibility QA — development acceptance checklist

This document scopes **M6.4** against Retivus's actual Rust/egui desktop UI. Automation is necessary but does not replace a screen-reader or assistive-technology evaluation. Keep unverified items pending.

## Automated coverage

| Contract | Test/evidence | Status |
| --- | --- | --- |
| Keyboard focus follows Tab and Shift+Tab | Native egui_kittest source-file focus and reversal test in src/main.rs | Verified in feature CI 41347374839 |
| Native pointer controls actually complete a review | Source validation, precise sense selection, card creation, grade and persistence via egui_kittest | Verified in feature CI 41347374839 |
| Note edit/search controls respond to keyboard/mouse | Native editor and search end-to-end regression in src/study_ui.rs | Verified in feature CI 41347374839 |
| System, Light and Dark persist correctly | App preference tests in src/main.rs | Verified in foundation CI |
| Scale, density, window resizing and labelled input focus | accessibility_focus_survives_scaling_density_theme_and_resizing | **Pending its own CI** |
| Real compositor renders wide and narrow windows | Retained native Sway Wayland 1080×800 and 720×650 screenshot artifact, reviewed for initial-window clipping and missing glyphs | Verified in Anki CI 41343238570 |

## Real-system visual and assistive-technology checks

- [x] X11 and Sway Wayland native windows launch in CI; initial-window screenshots inspected at multiple sizes
- [x] Real keyboard/pointer test navigates source entry selection, grading and persistence
- [ ] Inspect **populated** browse, source conflict and reviewer states at both scales in a physical Wayland session
- [ ] Verify screen-reader announcements of form labels, deck choices, status updates and due-card transitions with an actual screen reader
- [ ] Validate high-contrast mode and any reduced-motion platform preference against user testing

**Scope:** No automatic success or milestone credit for unchecked manual items. Check only observed evidence. The automated focus/resize regression must pass before M6.4 can be considered for acceptance, and broad assistive-technology support is not claimed here.
