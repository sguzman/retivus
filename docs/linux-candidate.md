# Linux release candidate (not published)

Run `cargo build --locked --release --features anki-backend --bin retivus`, then `bash scripts/package-linux.sh dist`. The candidate contains the binary, license, Anki attribution, original MIT notice, source snapshot, resolved Cargo.lock, desktop entry and local-prefix install/uninstall scripts.

The [candidate CI workflow](../.github/workflows/linux-candidate.yml) builds and checksums the archive, installs it into an isolated home **including AGPL, pinned Anki attribution, original MIT provenance and README**, launches a real X11 window, repeats installation, and uninstalls only when the managed binary, desktop entry and notice checksums match. The workflow also verifies that unmanaged binaries and existing notices cannot be overwritten. The lockfile is retained for possible repository inclusion.

**Not accepted:** the exact lockfile from the earlier green candidate workflow has been committed (6b1a3e5), and official Anki Desktop package interchange passed separately (CI 41368266062). This installer/notice hardening still requires release-candidate CI. Full upstream corresponding-source/license review, AnkiWeb sync, a clean-target matrix and final release acceptance remain unfinished. No production release or tag should be published based on this workflow alone.
