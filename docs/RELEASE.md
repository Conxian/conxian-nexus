# Release Discipline

This document defines the minimum release gates for Conxian Nexus.

## Release Principles

- **Fail closed over fail open** for proof, sync, and safety-critical paths.
- **Ownership before merge** for high-impact changes.
- **No boundary exceptions**: production boundary checks are mandatory.
- **Traceable artifacts**: version and changelog updates must match shipped behavior.

## Required Gates

A change is release-eligible only if all applicable gates pass.

1. **Ownership signoff**
   - Required review from [`.github/CODEOWNERS`](../.github/CODEOWNERS) owners for touched areas.

2. **CI hygiene and test gates**
   - [`.github/workflows/rust.yml`](../.github/workflows/rust.yml) passes in full.
   - Includes contamination, submodule integrity, and production boundary checks.

3. **Boundary integrity gate**
   - `./scripts/check_production_boundary.sh` must pass locally/CI for release candidates.

4. **Proof and integrity impact gate**
   - Any proof, MMR, sync, or safety behavior change must include targeted tests.
   - Regressions or partial-proof behavior are release blockers.

5. **Version and release notes consistency (when relevant)**
   - Public or operationally relevant changes must be reflected in:
     - [`CHANGELOG.md`](../CHANGELOG.md)
     - versioned metadata where applicable (for example API spec version in [`docs/openapi.yaml`](./openapi.yaml)).

## Release Checklist (Operator Quick-Run)

- [ ] Required CODEOWNERS approvals recorded.
- [ ] CI workflow green for release branch/commit.
- [ ] Boundary checks verified.
- [ ] Proof/integrity impacts assessed and tested.
- [ ] Changelog/version metadata updated where relevant.
- [ ] Release notes drafted with operational impact and rollback notes.

## Control Model Linkage

This release discipline is the execution gate layer for the repository control model in [`docs/CONTROL_MODEL.md`](./CONTROL_MODEL.md).
