# License compliance governance

This repository enforces dependency-license policy without asserting unresolved
terms for the Conxian Nexus package itself.

## Automated controls

- `deny.toml` is the SPDX-aware source, license, and duplicate-version policy.
- `.github/dependency-review-config.yml` applies the same permitted-license set
  to dependency diffs and keeps vulnerability review enabled.
- `scripts/check_dependency_licenses.py` fails on every dependency or workspace
  license-policy error reported by the pinned cargo-deny version. It does not
  convert unresolved legal terms into repository approval.
- `deny.toml` requires native Cargo git sources to use `rev`, and
  `scripts/check_dependency_declarations.py` additionally rejects wildcard
  versions and requires a full 40-character commit ID. The Python check is
  retained because Cargo and cargo-deny accept abbreviated revision values.
- `scripts/generate_compliance_artifacts.sh` generates a third-party license
  report and a normalized CycloneDX 1.5 SBOM under `target/compliance/`.
- `scripts/run_license_compliance.sh` is the shared policy/artifact entry point
  used by pull request, main, tag, and release workflows so controls cannot
  drift between publication paths.
- `.github/workflows/license-governance.yml` verifies policy and uploads those
  artifacts for review. The release workflow has a dedicated compliance gate;
  release creation, crates.io publication, and attestation all depend on it.

Tool versions are pinned in workflows. Generated artifacts are not committed;
they are derived from `Cargo.lock` and uploaded by CI/release jobs.
SBOM normalization removes generator timestamps/serials and recursively maps
absolute workspace paths in component references, PURLs, and dependency links
to `file:///workspace`. Deterministic hash claims assume the same `Cargo.lock`,
manifests, feature set, pinned generator versions, Rust target, and toolchain;
target-dependent dependency metadata is intentionally outside that scope.
The notice report verifies that the known git and Boost-licensed transitive
dependencies are present. Artifact generation remains fail closed when the
exact graph contains a license outside the repository-approved policy.

## Narrow dependency exceptions

`xxhash-rust 0.8.18` declares SPDX `BSL-1.0`, which is the permissive **Boost
Software License 1.0**, not the Business Source License (`BUSL-1.1`). It is an
unconditional dependency of `redis 1.4.1`; Redis 1.0.0 through 1.4.1 retain that
dependency, and feature disabling cannot remove it. Replacing or downgrading
Redis would change behavior/API surface, so no dependency change is justified.

The exception is exact-package-and-version in `deny.toml` and dependency
review. `about.toml` has a crate-specific reporting exception, and the artifact
script refuses to use it unless the resolved version is exactly `0.8.18`. There
is no repository-wide BSL allowlist.

The pinned `lib-conxian-core` revision
`fdd73046a97b53b1ede54d342b0439287dd44593` resolves as version `0.3.0` and
declares `MIT OR Apache-2.0`. Nexus verifies that exact locked source and does
not rewrite upstream metadata. This is the GitHub-verified merge commit from
Core pull request #237; its removal of the unused legacy BDK dependency closure
also removes the stale `webpki` and `webpki-roots` policy findings from Nexus.

No FIBO package is present in the resolved Cargo dependency graph. FIBO is
unrelated to this repository's current license failure.

## Owner and legal decisions still required

Root Cargo legal and distribution metadata remains outside these mechanical
controls. Repository automation does not select or alter license terms,
licensor identity, publication posture, or related legal metadata.

The current exact graph also exposes license-policy decisions that this
mechanical remediation does not make: the workspace root is detected as
`BUSL-1.1`, and `hex_lit 0.1.1` declares `MITNFA`. The compliance entry point
therefore fails closed until authorized policy owners resolve those terms.

GitHub ruleset `19543903` and its external `license_compliance_scanning` policy
remain an enterprise/repository administrator responsibility. These repository
controls do not configure, bypass, rename, or weaken that enforcement. An
authorized administrator must configure the external license policy after the
licensor completes the root terms.

## Local verification

```bash
python3 scripts/check_dependency_licenses.py
python3 scripts/check_dependency_declarations.py
cargo deny --locked check bans sources --hide-inclusion-graph
scripts/generate_compliance_artifacts.sh
python3 scripts/test_normalize_sbom.py
```
