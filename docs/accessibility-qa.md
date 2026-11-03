# Native accessibility QA — development acceptance checklist

This document scopes **M6.4** against Retivus's actual Rust/egui desktop UI. Automation is necessary but does not replace a screen-reader or assistive-technology evaluation. Keep unverified items pending.

## Native platform accessibility bridge

Retivus disables eframe default features to control its native build. Its explicit `accesskit` feature is now enabled in `Cargo.toml` alongside X11 and Wayland so the application can expose its standard egui widget tree to supported Linux AT-SPI assistive technology. This is a **build-time prerequisite**, not evidence that Orca actually announced content correctly. A real accessibility bus/screen-reader test and dynamic status announcements still need verification; do not mark M6.4 complete just because the feature compiles.

## Automated coverage

| Contract | Test/evidence | Status |
| --- | --- | --- |
| AccessKit platform accessibility implementation compiled into native app | Explicit `eframe/accesskit`, [foundation CI 41370243289](https://github.com/sguzman/retivus/actions/runs/41370243289) | **Verified build, not screen-reader usability** |
| Test-only Orca speech-debug and keyboard focus | [Orca CI 41371981361](https://github.com/sguzman/retivus/actions/runs/41371981361) verified spoken `Appearance`, `Density`, `Format`, `File` and `Validate source` during real keyboard navigation | **Verified named-control announcements; not full assistive-technology usability** |
| Populated learner/reviewer/conflict keyboard and scaling regression | `populated_native_review_and_conflict_remain_accessible_across_zoom_and_themes` in `src/study_ui.rs` uses a real disposable Anki profile, Enter grading, 1080×800 and 720×650 at 1.5× zoom, and a malformed source tag conflict state | **Unified E2E CI 41471532608 passed; latest pinned feature CI pending** |
| Dynamic review, due-count, conflict and source-validation status nodes | Polite AccessKit live-region metadata in `src/study_ui.rs` and `src/main.rs`; [foundation CI 41469642651](https://github.com/sguzman/retivus/actions/runs/41469642651) passed | **Foundation CI 41469642651 and E2E CI 41469642552 passed; automatic spoken Orca announcement still pending** |
| Real native Linux AT-SPI widget registration | [AT-SPI CI 41371103522](https://github.com/sguzman/retivus/actions/runs/41371103522) used an external client to discover `File` and `Validate source` among 23 accessible objects | **Verified native registration** |
| Keyboard focus follows Tab and Shift+Tab | Native egui_kittest source-file focus and reversal test in src/main.rs | Verified in feature CI 41347374839 |
| Native pointer controls actually complete a review | Source validation, precise sense selection, card creation, grade and persistence via egui_kittest | Verified in feature CI 41347374839 |
| Note edit/search controls respond to keyboard/mouse | Native editor and search end-to-end regression in src/study_ui.rs | Verified in feature CI 41347374839 |
| System, Light and Dark persist correctly | App preference tests in src/main.rs | Verified in foundation CI |
| High-contrast and reduced-motion controls (including disabled scroll transitions) | Persisted native preferences and light/dark override/reset tests in `src/main.rs` | **Implementation committed; CI and real-user visual QA pending** |
| Scale, density, window resizing, labelled input focus and Enter-activated validation | `accessibility_focus_survives_scaling_density_theme_and_resizing` in `src/main.rs` | **Verified foundation CI 41371103471** |
| Real compositor renders wide and narrow windows | Retained native Sway Wayland 1080×800 and 720×650 screenshot artifact, reviewed for initial-window clipping and missing glyphs | Verified in Anki CI 41343238570 |

## Real-system visual and assistive-technology checks

- [x] X11 and Sway Wayland native windows launch in CI; initial-window screenshots inspected at multiple sizes
- [x] Real keyboard/pointer test navigates source entry selection, grading and persistence
- [ ] Inspect **populated** browse, source conflict and reviewer states at both scales in a physical Wayland session. The separate disposable debug-only `RETIVUS_VISUAL_QA=isolated-2026` fixture and Sway screenshots are in acceptance testing; a headless compositor does not replace an independently inspected physical workstation.
- [ ] Verify screen-reader announcements of form labels, deck choices, status updates and due-card transitions with an actual screen reader
- [ ] Inspect High contrast and Reduce motion controls in light/dark and narrow/wide windows; the new settings persist, but OS-level reduced-motion autodetection and physical Wayland usability have not been verified

**Scope:** Native AT-SPI registration, keyboard/scale/theme focus regression, and **Orca announcements for the named core controls** have passed CI. **M6.4 remains pending (0/3 points)** because populated browse/reviewer/conflict states in a physical Wayland session, broader screen-reader status/due-card announcements, and high-contrast/reduced-motion preferences have not been independently accepted. The tests do not claim universal assistive-technology usability.


## 2026-11-03 independent GTK/Orca control (no M6.4 credit)

The disposable Xvfb/Openbox/DBus Orca test was run against **unrelated GTK 4 software**, `zenity --question`, with two explicitly labelled native buttons. The GTK test uses the same Orca + AT-SPI stack as Retivus, but **no Retivus binary, AccessKit source, or learner profile**.

- [GTK baseline 41564407441](https://github.com/sguzman/retivus/actions/runs/41564407441): the dialog opened and AT-SPI observed `Reference OK` and `Reference Cancel` focus transitions. Orca spoke `Screen reader on` and `tab`, but not either button name. The trace said the GTK dialog `lacks active state` and no active window could be found.
- [Reordered startup 41564579029](https://github.com/sguzman/retivus/actions/runs/41564579029): launching and activating GTK **before** Orca still failed to produce named-control speech. The same missing active-window state appeared, while button focus events were delivered.
- [Retivus normal-status CI 41562902256](https://github.com/sguzman/retivus/actions/runs/41562902256) and [assertive-alert CI 41563452566](https://github.com/sguzman/retivus/actions/runs/41563452566): status/notification events reached Orca but did not produce required status speech. The alert experiment has been retired; the real production widget remains an atomic, polite `Status`.

**Interpretation:** This reproduces a *similar failure to speak named focused controls outside Retivus*, so the synthetic test environment is not sufficient to identify a Retivus-specific root cause. It does **not** prove the application accessible, and does not allow downgrading the strict Orca acceptance assertion. The independent GTK workflow is a **manual diagnostic only** after these two runs; ordinary CI should not repeatedly run this known-failing environmental control.

**Remaining acceptance:** a populated physical Wayland + Orca session, dynamic status/due-card speech, high contrast, reduced motion, and both viewport sizes. Do not mark M6.4 completed on Xvfb speech-event evidence alone.

## 2026-11-03 independent native Wayland Orca control

The separate diagnostic runs **native GTK4 Zenity on headless Sway with Xwayland disabled** and injects real Wayland virtual-keyboard Tab events. It contains no Retivus code or AccessKit widgets. This is a CI environment diagnostic, not a substitute for physical Wayland acceptance.

- [First Sway run 41566851064](https://github.com/sguzman/retivus/actions/runs/41566851064): the GTK dialog rendered and its screenshot was retained. Ubuntu 24.04 Orca 46.1 **crashed during startup** with `KeyError: DISPLAY` in `orca_modifier_manager.py`, which invokes X11 `xkbcomp` even under `XDG_SESSION_TYPE=wayland`. It was therefore **not a valid screen-reader speech test**.
- [Corrected Sway run 41567126705](https://github.com/sguzman/retivus/actions/runs/41567126705): a disposable sidecar Xvfb `DISPLAY` was provided solely for Orca's legacy keyboard-map initialization. `GDK_BACKEND=wayland`, `xwayland disable`, native Sway and Wayland `wtype` remained in place. Orca started; its log recorded real GTK button focus events, yet failed to speak `Reference OK` or `Reference Cancel`, repeatedly reporting `Active window is None`. The retained screenshot shows both buttons in the native Wayland GTK dialog.
- Earlier [independent X11 baseline 41564407441](https://github.com/sguzman/retivus/actions/runs/41564407441) and [reversed startup 41564579029](https://github.com/sguzman/retivus/actions/runs/41564579029) also lacked named-control speech from ordinary GTK.

**Interpretation:** Both virtual display backends can render and receive accessibility events while failing this unrelated GTK spoken-name control. This strengthens the case for a **virtual-session/Orca active-window limitation** and prevents attributing the same symptom exclusively to Retivus. It does not prove that Retivus's live announcements are correct under a full real desktop session. The Wayland diagnostic is now `workflow_dispatch` only rather than an automatically failing CI gate. The strict Retivus-specific speech acceptance is unchanged and remains red; M6.4 physical Wayland + Orca, dynamic due/status, populated screens, contrast/motion checks remain pending.


## 2026-11-03 populated Wayland narrow-toolbar QA (visual defect closed, M6.4 pending)

[Anki feature workflow 41667670187](https://github.com/sguzman/retivus/actions/runs/41667670187) passed its native X11 and Sway Wayland UI stages and retained separate native Wayland review/conflict screenshots at **1080×800 and 720×650**. The screenshots were independently opened and inspected, not merely accepted on file size.

The earlier [run 41567925894](https://github.com/sguzman/retivus/actions/runs/41567925894) showed a blank unpainted second toolbar band in populated narrow Light/High-contrast reviewer and Dark/High-contrast conflict states. The later [`58a1b91`](https://github.com/sguzman/retivus/commit/58a1b914db59b124a6929164ce77fd3ceda95e57) revision moves both narrow `High contrast` and `Reduce motion` toggles from the clipped top panel into the top of the central panel. Both controls **are visibly painted and checked** in the new retained Wayland review and conflict screenshots at 720×650; the blank band is gone. The wide layouts retain their top-row controls.

The [foundation regression 41570073956](https://github.com/sguzman/retivus/actions/runs/41570073956), [feature-enabled E2E 41570073728](https://github.com/sguzman/retivus/actions/runs/41570073728), and [four-job Linux candidate 41570073802](https://github.com/sguzman/retivus/actions/runs/41570073802) passed on the same application revision. The **separately downloaded** Linux candidate was SHA256-verified and launched in disposable Xvfb/Openbox with 720×650 and wide window screenshots; clicking High contrast visibly switched the palette while retaining both controls. In one local test Openbox was not ready when the application was launched and no managed window appeared; waiting for its EWMH readiness then produced a mapped Retivus window. The [X11 workflow readiness fix `b99b347`](https://github.com/sguzman/retivus/commit/b99b3479335c0bb7705cf7af9b5655050b174d43) now waits for Openbox instead of forcing a hidden application window to map.

**Boundary:** This verifies the specific narrow-toolbar painting defect in retained native Sway screenshots and a separate packaged X11 run. It is **not** a physical Wayland workstation assessment, and does not demonstrate live Orca speech for Retivus: the actual Orca changed-status requirement remains failed; M6.4 stays pending at 0/3.

