# Anki Rust backend — upstream attribution and integration notice

This notice applies to Retivus builds using the optional `anki-backend` Cargo feature.

- **Upstream project:** [Anki](https://github.com/ankitects/anki) by **Ankitects Pty Ltd and contributors**.
- **Reused component:** Anki's Rust `rslib` package, whose Cargo crate is named `anki`.
- **Pinned upstream source:** [commit `136bd42bfbd356fc756b6d3e28cc77b02da11ea7`](https://github.com/ankitects/anki/commit/136bd42bfbd356fc756b6d3e28cc77b02da11ea7).
- **Upstream license:** **GNU AGPL-3.0-or-later**; see [Anki's full repository LICENSE](https://github.com/ankitects/anki/blob/136bd42bfbd356fc756b6d3e28cc77b02da11ea7/LICENSE) and the [AGPLv3 text](../LICENSE).
- **Retivus combined-application license:** **AGPL-3.0-or-later**. Original pre-transition Retivus versions were MIT-licensed; see [historical MIT notice](MIT-original.txt).

Retivus fetches the upstream Rust crate from its public Git repository using the exact `rev` in the root `Cargo.toml`. It does **not** vendor, rebrand, or claim ownership of upstream Anki code. The current integration does not patch upstream source; a direct Tokio `io-util` feature is requested from Retivus to satisfy the pinned upstream Rust feature requirements. Earlier **research-only** CI experiments used an isolated, temporary upstream manifest adjustment to achieve the same effect; those temporary changes were not submitted to upstream Anki or distributed with Retivus.

## Before distributing a feature-enabled build

1. Record the exact dependency graph and resolved source revisions, including Cargo lockfile and third-party notices.
2. Ensure recipients can obtain the complete corresponding source for the combined binary (Retivus and covered upstream components), with the appropriate build/installation instructions.
3. Retain upstream credits and required notices in redistributed packages; identify any actual upstream source modifications if they are ever made.
4. Test reproducible builds and applicable AGPL obligations, including network-use obligations where relevant.
5. Keep independently licensed vocabulary, external media, and private personal review databases separate from program-source licensing.

This attribution document is the **source-level notice** for the feature integration. It does not itself certify a final distributable as compliant, and is not a substitute for the M7 packaging verification gate.
