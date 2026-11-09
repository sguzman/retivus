# Independent release reviewer handoff (M7) — pending

The [M7.1 release-notice issue](https://github.com/sguzman/retivus/issues/2)
requires an independent decision about the exact distributed artifacts, not
an inference from green CI. The separate physical Wayland/Orca screen-reader
acceptance in [accessibility-qa.md](accessibility-qa.md) remains required.

## Unsigned reviewer packet

A successful [Linux candidate acceptance](../.github/workflows/linux-candidate.yml)
run now uploads an additional release-review packet, produced only AFTER the
independent binary/source parity checks pass. It is tied to the same full Git
commit and the same two checksummed binary and vendored-source candidate
archives. The packet records both SHA-256 archive digests, Cargo.lock hash,
source audit, and dependency/license provenance inventories.

The included STATUS.txt intentionally says NOT REVIEWED - DO NOT RELEASE.
The packet is not a compliance certificate or release authorization.
The full archives are separate GitHub Actions artifacts: the reviewer must
download both, independently calculate their hashes and inspect their
actual contents. The binary, corresponding source and reviewer packet artifacts are retained
for 30 days by the release workflows (subject to repository retention policy).
Download and preserve all three together before they expire; GitHub Actions
artifacts are temporary evidence, not permanent project archives.

## Required independent decisions

1. Inspect the exact binary/source archives, AGPL terms, original MIT notice,
   pinned Anki terms and contributors, plus build and install scripts.
2. Review actually incorporated Linux dependencies and all-platform source
   obligations; a Cargo dependency graph is not itself a linking audit.
3. Decide notice sufficiency for dispatch 0.2.0 author-companion MIT evidence,
   htmlescape 0.3.1 Debian attribution, hexf-parse 0.2.1 Debian CC0 evidence,
   other parent/workspace notices and external provider data rights.
4. Check independent runtime, installation, update, recovery and data safety
   results against those exact archives (M7.2 and M7.3).
5. Review separate personally witnessed physical Wayland/Orca evidence (M6.4).
6. Record independent reviewer identity, UTC date, independently rechecked
   hashes, remaining exceptions and explicit approval or refusal. Only then
   may the owner make an M7.4 version/tag/publication decision.

No roadmap credit or approval is earned by generating this packet. Nothing
in this document authorizes a tag, production release or legal disposition.

The packet writer is `scripts/prepare-release-review.sh`; its fail-closed archive and unsigned-status behavior is checked using *synthetic* fixtures by `scripts/test-prepare-release-review.sh` in the Linux installer contract workflow. This test does not replace real artifact parity, the independent offline-source rebuild, or any human acceptance gate.
