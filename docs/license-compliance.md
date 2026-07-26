# License compliance governance

This repository enforces dependency-license policy without asserting unresolved
terms for the Conxian Nexus package itself.

## Automated controls

- `deny.toml` is the SPDX-aware source, license, and duplicate-version policy.
- `.github/dependency-review-config.yml` applies the same permitted-license set
  to dependency diffs and keeps vulnerability review enabled.
- `scripts/check_dependency_licenses.py` fails on any dependency license error.
  It tolerates only cargo-deny's exact error for the unlicensed workspace root,
  which remains a visible legal blocker rather than an approval.
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
The notice report intentionally omits the first-party workspace root because
its legal terms are unresolved; the script verifies that the known git and
Boost-licensed transitive dependencies are present in the generated report.

## Narrow dependency exceptions

`xxhash-rust 0.8.15` declares SPDX `BSL-1.0`, which is the permissive **Boost
Software License 1.0**, not the Business Source License (`BUSL-1.1`). It is an
unconditional dependency of `redis 1.4.1`; Redis 1.0.0 through 1.4.1 retain that
dependency, and feature disabling cannot remove it. Replacing or downgrading
Redis would change behavior/API surface, so no dependency change is justified.

The exception is exact-package-and-version in `deny.toml` and dependency
review. `about.toml` has a crate-specific reporting exception, and the artifact
script refuses to use it unless the resolved version is exactly `0.8.15`. There
is no repository-wide BSL allowlist.

The pinned `lib-conxian-core` revision contains a complete MIT license file,
which cargo-deny detects, but its manifest lacks `license` metadata. Its owners
should add accurate metadata upstream; this repository does not rewrite it.

No FIBO package is present in the resolved Cargo dependency graph. FIBO is
unrelated to this repository's current license failure.

## Owner and legal decisions still required

The root `LICENSE` is a six-line BUSL 1.1 placeholder. An authorized licensor
must provide the complete license text and decide the Change Date, Change
License, and any Additional Use Grant. Only after that review should an owner
authorize Cargo `license` or `license-file` metadata. Repository automation must
not choose those legal terms.

`scripts/detect_placeholder_license.py` only detects the known placeholder and
intentionally fails with that remediation. A passing result means only that the
known six-line placeholder was not detected; it is not a legal validity,
completeness, ownership, or Cargo metadata assessment. It is staged for owner
verification but is not in CI while the known placeholder remains, avoiding an
unrelated permanently-red main branch.

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
python3 scripts/detect_placeholder_license.py # expected to fail until owner action
```
