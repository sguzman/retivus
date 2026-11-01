# Linux release candidate (not published)

Run `cargo build --locked --release --features anki-backend --bin retivus`, then `bash scripts/package-linux.sh dist`. The candidate contains the binary, license, Anki attribution, original MIT notice, source snapshot, resolved Cargo.lock, desktop entry and local-prefix install/uninstall scripts.

The [candidate CI workflow](../.github/workflows/linux-candidate.yml) builds and checksums the archive, installs it into an isolated home, launches a real X11 window, repeats installation, and uninstalls only when the tracked binary checksum matches. The lockfile is retained for possible repository inclusion.

**Not accepted:** Cargo.lock is not yet committed; full corresponding-source/license review, official Anki Desktop import/reexport, a clean-target matrix and final release acceptance remain unfinished. No production release or tag should be published based on this workflow alone.
