# Independent Anki Desktop GUI acceptance

The [separate workflow](../.github/workflows/anki-desktop-interop.yml) downloads the SHA-256-pinned official 26.09.3 Linux Desktop, and completes the Retivus fixture's import and reexport through the **actual Desktop windows** under virtual X11. HOME and ANKI_BASE are temporary; no personal Anki profile, password, or account is involved.

It captures Anki's import results and five-card deck screen, exercises Export/Save in the GUI, and independently reimports the **Desktop-produced** `.apkg` in Retivus to verify exact four-note/five-card counts, fields, deck and two-card note type. The acceptance gate does not rely on reading Anki's live SQLite database while the GUI has it open.

**Status: accepted for M4.2.** [CI 41368266062](https://github.com/sguzman/retivus/actions/runs/41368266062) passed and its [GUI screenshots](https://github.com/sguzman/retivus/actions/runs/41368266062/artifacts/13108466264) were inspected. This is package interoperability, not AnkiWeb sync. The shell driver is development-only; it adds no Python/Qt runtime dependency to Retivus.