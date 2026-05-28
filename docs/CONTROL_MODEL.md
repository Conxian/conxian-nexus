# Conxian Nexus Control Model

## Mission and Scope Boundaries

Conxian Nexus is a **technical synchronization and proof service** for Conxian systems. Its scope is to:

- ingest and synchronize canonical chain events,
- maintain verifiable state and MMR structures,
- expose proof and verification APIs,
- enforce safety-mode controls when integrity checks fail.

Nexus does **not** define business policy. It enforces deterministic technical controls and exposes auditable outputs.

## Explicit Non-Goals

This repository must **not** own:

- protocol governance authority,
- treasury/settlement policy authority,
- business execution authority,
- off-repo product/risk policy decisions.

Those decisions live in parent governance and upstream control systems. Nexus implements approved policy as code; it does not originate policy.

## Ownership and Control Points

### Code ownership

- Canonical ownership is defined in [`.github/CODEOWNERS`](../.github/CODEOWNERS).
- Changes touching proof paths, execution logic, state sync, or safety controls require owner review.

### CI boundary checks

Required guardrails are enforced in CI via [`.github/workflows/rust.yml`](../.github/workflows/rust.yml):

- contamination guard: [`scripts/verify_contamination_guard.py`](../scripts/verify_contamination_guard.py)
- submodule integrity check: [`scripts/verify_submodule_integrity.py`](../scripts/verify_submodule_integrity.py)
- production boundary check: [`scripts/check_production_boundary.sh`](../scripts/check_production_boundary.sh)

Any failure is a release blocker.

### PR control surface

- PRs must satisfy the repository checklist in [`.github/PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md), including boundary and integrity declarations.

## Operational Support Expectations

Contributors and maintainers are expected to:

- preserve fail-closed behavior for proof and safety-critical APIs,
- document any operationally significant behavior changes,
- prioritize incident triage for state/proof correctness regressions,
- keep migrations and API behavior backward-compatible unless explicitly approved.

## Release Discipline Gates

No release is considered ready unless required ownership, CI, boundary, and integrity gates pass.

See [Release Discipline (`docs/RELEASE.md`)](./RELEASE.md) for the mandatory gate sequence.

## Related Governance References

- [README Governance & Security](../README.md#governance--security)
- [Contributing Guide](../CONTRIBUTING.md)
- [Security Policy](../SECURITY.md)
- [Mainnet Readiness Evidence](./MAINNET_EVIDENCE.md)
- [Product Requirements Document](./PRD.md)
