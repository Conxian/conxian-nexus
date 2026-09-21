# Conxian Nexus Release Governance Runbook

This document details the release process, versioning rules, governance sign-offs, and automated CI pipeline controls for releasing `conxian-nexus`.

## Versioning & Tagging Policy

- **Semantic Versioning**: Follow `MAJOR.MINOR.PATCH` (e.g., `0.4.23`).
- **Tag Format**: Release tags must follow `vMAJOR.MINOR.PATCH` (e.g., `v0.4.23`).
- **MSRV Alignment**: Releases require Rust 1.98.1 (MSRV) as specified in `Cargo.toml` and `.github/workflows/release.yml`.
- **Version Synchronization**: The version in `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md` must be identical prior to creating a release tag.

## Changelog Requirements

- Update `CHANGELOG.md` before creating a release tag.
- Add an explicit `## [X.Y.Z] - YYYY-MM-DD` section detailing:
  - `Added`: New endpoints, verifiers, or capabilities.
  - `Changed`: Upgraded dependencies, refactored interfaces, or documentation updates.
  - `Fixed`: Bug fixes or security hardings.

## Automated CI Release Pipeline

Releases are processed through the 7-stage automated workflow in `.github/workflows/release.yml`:

| Stage | Name | Key Controls & Checks |
|-------|------|----------------------|
| **1** | **Hygiene & Contamination Guard** | Gitleaks secret scanning (v8.18.2 with SHA-256 validation), `verify_contamination_guard.py`, `check_production_boundary.sh`, and workflow YAML verification. |
| **2** | **Build, Test & Scoped Coverage** | Cargo workspace build/test execution under Rust 1.98.1; enforcing Lightning coverage (≥90%) and Bitcoin coverage (≥92%). |
| **3** | **Version Validation & Notes Extraction** | Ensures tag version matches `Cargo.toml` `version` field; parses release notes directly from `CHANGELOG.md`. |
| **4** | **Release License Compliance Gate** | Executes `scripts/run_license_compliance.sh` using pinned tools (`cargo-deny` 0.18.6, `cargo-about` 0.8.2, `cargo-cyclonedx` 0.5.7) to verify software licenses and generate SBOM/attestation artifacts in `target/compliance/`. |
| **5** | **Create GitHub Release** | Automatically creates the GitHub Release with extracted changelog notes and prerelease flags if applicable. |
| **6** | **Publish to crates.io** | Automatically publishes the crate to `crates.io` using `CARGO_REGISTRY_TOKEN` upon git tag push. |
| **7** | **Generate SLSA Attestation** | Generates SLSA build provenance attestations for the compiled release binary (`./target/release/conxian-nexus`). |

## Release Step-by-Step Procedure

1. **Prepare Release Branch / PR**:
   - Verify all feature PRs are merged into `main`.
   - Update `Cargo.toml` and `Cargo.lock` version.
   - Update `CHANGELOG.md` with release version, date, and categorised notes.
   - Obtain required CODEOWNERS sign-off on release PR.

2. **Local Pre-Release Verification**:
   ```bash
   # 1. Clean build & test suite
   cargo build --workspace --locked
   cargo test --workspace --locked

   # 2. Production boundary & contamination checks
   python3 scripts/verify_contamination_guard.py
   ./scripts/check_production_boundary.sh
   python3 scripts/check_dependency_declarations.py
   ```

3. **Tag & Push Release**:
   ```bash
   git checkout main
   git pull origin main
   git tag -a vX.Y.Z -m "Release vX.Y.Z"
   git push origin vX.Y.Z
   ```

4. **Monitor Release Pipeline**:
   - Track workflow execution under GitHub Actions (`.github/workflows/release.yml`).
   - Confirm all 7 release stages pass successfully.

## Pre-Release Control Checklist

- [ ] Version in `Cargo.toml` matches tag `vX.Y.Z`.
- [ ] `CHANGELOG.md` contains an entry for `[X.Y.Z]` with matching date and notes.
- [ ] State-proof and synchronization adapters reviewed for multi-chain safety.
- [ ] Production storage boundary (remote authenticated Postgres/Redis) verified.
- [ ] No secrets, private operational data, or forbidden testnet keys introduced.
- [ ] CODEOWNERS review obtained for governance and release changes.
- [ ] GitHub Release, crates.io package, and SLSA attestation generated upon pipeline completion.
