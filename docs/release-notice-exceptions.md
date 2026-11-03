# M7.1 — unresolved upstream notice exceptions

This is a verified release-blocker ledger, **not** a legal opinion or a
license-compliance certificate. The source archive carries the complete locked
crate sources and their manifests. Some older upstream projects published a
license declaration without a standalone license/copyright notice.

The [parent-notice audit](../scripts/audit-parent-notices.sh) must keep the
following entries at `REVIEW_PARENT_OR_UPSTREAM` until the actual
corresponding notices or a documented, authorized legal disposition are
obtained. The `PARENT-NOTICE-REVIEW.tsv` inventory is generated at build
time and takes precedence if the dependency graph changes.

| Crate | Cargo license declaration | Exact published source evidence | Outstanding proof |
| --- | --- | --- | --- |
| `dispatch 0.2.0` | `MIT` | Cargo VCS commit `82d6c7a5b75dc0c71c3f46f87bb6c16a476f7748` in [SSheldon/rust-dispatch](https://github.com/SSheldon/rust-dispatch/tree/82d6c7a5b75dc0c71c3f46f87bb6c16a476f7748) | No upstream standalone license notice in that revision; obtain original permission/copyright text or supported disposition |
| `hexf-parse 0.2.1` | `CC0-1.0` | Cargo VCS commit `4225763d744183d720f575ae96d04161b4d08ea0` in [lifthrasiir/hexf](https://github.com/lifthrasiir/hexf/tree/4225763d744183d720f575ae96d04161b4d08ea0) | Preserve authoritative CC0 declaration and determine appropriate downstream notice treatment |
| `htmlescape 0.3.1` | `Apache-2.0 / MIT / MPL-2.0` (*as published*, not a normalized SPDX expression) | All 11 published project files exactly match upstream [commit `1699b539`](https://github.com/veddan/rust-htmlescape/tree/1699b539179798e705ad8464128492a0a0092876); no Cargo VCS metadata in the crate | Clarify the intended license alternatives, copyright holders, and actual legal text, including any MPL file-level obligations |
| `intl_pluralrules 7.0.2` | `Apache-2.0/MIT` (*as published*) | Cargo VCS commit `d0715d5b7cca72e228d6e6c4de154bd30dfdd06a`, path `intl_pluralrules`, from [zbraniecki/pluralrules](https://github.com/zbraniecki/pluralrules/tree/d0715d5b7cca72e228d6e6c4de154bd30dfdd06a) | Identify corresponding upstream copyright/license text or approved provenance for the workspace publication |

**Verification:** the audit checks the published Cargo license expression, tracked
revision where present, and every `htmlescape` source file against the immutable
upstream commit. It leaves that crate unresolved rather than inferring license
compliance from source identity.

**Release rule:** M7.1 remains pending. A passing source audit, successful Linux
runtime install, or inclusion of generic standard license texts alone cannot
close these exceptions or certify the binary's notice obligations. Review which
packages are actually incorporated in each platform-specific binary separately
from their appearance in the all-platform Cargo lockfile.
