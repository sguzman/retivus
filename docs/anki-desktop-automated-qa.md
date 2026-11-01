# Independent Anki Desktop GUI acceptance

The [separate workflow](../.github/workflows/anki-desktop-interop.yml) downloads the SHA-256-pinned official 26.09.3 Linux Desktop, then attempts the Retivus fixture's import and reexport through the **actual Desktop windows** under virtual X11. HOME and ANKI_BASE are temporary; no personal Anki profile, password, or account is involved.

It checks the official collection for four notes and five cards, captures GUI screenshots, and must reimport the Desktop-produced .apkg into Retivus with preserved exact fields, deck and two-card note type.

**Status: not accepted.** Milestone M4.2 stays pending until this independent workflow is green and its captured GUI evidence is inspected. The shell driver is development-only; it adds no Python/Qt runtime dependency to Retivus.