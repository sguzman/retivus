# M7.1 — evidence requiring final attribution review

This is a verified release-blocker ledger, **not** a legal opinion or a
license-compliance certificate. The source archive carries the complete locked
crate sources and their manifests. Some older upstream projects published a
license declaration without a standalone license/copyright notice.

The [parent-notice audit](../scripts/audit-parent-notices.sh) preserves one
**author-companion** case separately from direct upstream license evidence.
This is not a legal disposition of the remaining original-source gap. The `PARENT-NOTICE-REVIEW.tsv` inventory is generated at build
time and takes precedence if the dependency graph changes.

| Crate | Cargo license declaration | Exact published source evidence | Outstanding proof |
| --- | --- | --- | --- |
| `dispatch 0.2.0` | `MIT` | Cargo VCS commit `82d6c7a5b75dc0c71c3f46f87bb6c16a476f7748` and exact original-author MIT text from [SSheldon/rust-objc](https://github.com/SSheldon/rust-objc/blob/c8696b0cd1932fdf1b5b22c03af86f32a934f42f/LICENSE.txt) | The text is from an author-controlled *companion* project, not a LICENSE shipped in `rust-dispatch`. Verify sufficiency for redistribution before M7.1 acceptance |

**Verification:** the audit checks the published Cargo license expression, tracked
revision where present, and every `htmlescape` source file against the immutable
upstream commit. It records that crate as **independently evidenced but not release-cleared**,
because source identity alone is not proof of license compliance.

**Release rule:** M7.1 remains pending. A passing source audit, successful Linux
runtime install, or inclusion of generic standard license texts alone cannot
close these exceptions or certify the binary's notice obligations. Review which
packages are actually incorporated in each platform-specific binary separately
from their appearance in the all-platform Cargo lockfile.

**Resolved source-evidence acquisition:** `intl_pluralrules 7.0.2` was removed from this exception list because its exact upstream workspace commit contains real license files in `intl_pluralrules/LICENSE-MIT` and `intl_pluralrules/LICENSE-APACHE`. Both were included in the source candidate under `LICENSES/upstream/intl_pluralrules-7.0.2/` and hash-verified. They were simply absent from the published crate archive. This does **not** certify the distribution as a whole.

**Independent Linux-target attribution acquired:** `htmlescape 0.3.1` is the sole previously unresolved crate reachable via the Linux normal/build graph. Debian's [source-package copyright record](https://metadata.ftp-master.debian.org/changelogs//main/r/rust-htmlescape/rust-htmlescape_0.3.1%2Bdfsg-1_copyright) for `0.3.1+dfsg-1` identifies copyright **2013–2017 Viktor Dahl**, interprets the historical Cargo license syntax as **Apache-2.0 OR MIT OR MPL-2.0**, and supplies MIT license terms. Retivus now preserves that copyright and full MIT alternative in [its independent packaging attribution](../LICENSES/upstream/htmlescape-0.3.1/DEBIAN-PACKAGING-ATTRIBUTION.md). Its published source is still byte-matched to the original upstream revision, but the origin lacks a LICENSE file: this evidence is classified as `DEBIAN_PACKAGING_NOTICE_PRESENT`, **not** `PINNED_PARENT_NOTICES_PRESENT` or a legally approved release. Any broader upstream, trademark, media, and binary-distribution duties remain for final M7.1 review. The other two cases are outside this Linux target's normal/build graph; that observation does not erase cross-platform source obligations.

**Independent CC0 evidence acquired:** Debian's [rust-hexf-parse 0.2.1-1 copyright record](https://metadata.ftp-master.debian.org/changelogs//main/r/rust-hexf-parse/rust-hexf-parse_0.2.1-1_copyright) names Kang Seonghoon (2017–2023) and CC0-1.0, matching the pinned upstream Cargo declaration. The candidate also includes the canonical `CC0-1.0` text from SPDX and a curated attribution under `LICENSES/upstream/hexf-parse-0.2.1/`; both are hash-checked, not presented as original upstream LICENSE files. Its status is `DEBIAN_PACKAGING_NOTICE_PRESENT`, not legal clearance. The source remains in the full vendored bundle even though it is not Linux normal/build-reachable.

**Author-companion source evidence added:** `LICENSES/upstream/dispatch-0.2.0/AUTHOR-COMPANION-MIT.txt` is byte-identical to Steven Sheldon's MIT notice in `SSheldon/rust-objc` (original Git blob `e9ab3babc0d8602b83dccde8a462f8f3dc1e52ad`). The corresponding `COMPANION-PROVENANCE.md` records the distinction. The generated status `AUTHOR_COMPANION_NOTICE_PRESENT` denotes acquired evidence **only**, and is explicitly not an original upstream license file or legal sign-off. The complete cross-platform source archive retains this dependency despite its absence from the Linux normal/build graph.

**Unsigned independent review handoff:** The Linux candidate's exact binary/source parity job now retains a reviewer packet with archive SHA-256 hashes and notice inventories. See [release-review-handoff.md](release-review-handoff.md). This organizes evidence only; the exceptions and M7.1 acceptance remain open.
