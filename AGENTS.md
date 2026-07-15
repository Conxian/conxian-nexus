# Conxian Nexus - Agent Knowledge Base

## Repository Overview

**Conxian Nexus** (aka Glass Node) is a protocol-first proof layer providing:
- Verifiable synchronization for Tier 1 Chain Families
- Multi-chain state normalization (Bitcoin, EVM, Cosmos)
- MMR (Merkle Mountain Range) state root commitments
- SRL-1 Lightning Network resilience layer

**Current Version**: v0.4.19
**Rust Version**: 1.82+
**License**: BSL 1.1

## Architecture

### Core Modules

| Module | Purpose |
|--------|---------|
| `src/api/` | REST/gRPC API surfaces (admin, analytics, billing, dlc, erp, grpc, identity, rest, security, settlement, zkml) |
| `src/executor/` | Multi-chain adapters: BitVM2, Cosmos, EVM, Fedimint, Lightning, RGB, Stacks |
| `src/oracle/` | Oracle service for Stacks contract integration |
| `src/orchestrator/` | Autonomous orchestrator for self-healing (SRL-1 Lightning recovery) |
| `src/safety/` | Safety mode enforcement & drift monitoring |
| `src/state/` | MMR state root commitments |
| `src/storage/` | Kwil & Tableland persistence adapters |
| `src/sync/` | Multi-chain ingestion and reorganization handling |

### External Dependencies

- **lib-conxian-core**: Git dependency from `https://github.com/Conxian/lib-conxian-core` (rev: 3b091d27...)
  - Used for: Wallet, Bitcoin primitives, RGB, Lightning
  - Current version: 0.2.12

## Release Process

### Version Alignment Requirements

⚠️ **CRITICAL**: Version alignment must be maintained across:
1. `Cargo.toml` version field
2. Git tag (e.g., `v0.4.19`)
3. GitHub Release
4. CHANGELOG.md entries

### Release Workflow (Automated)

The `.github/workflows/release.yml` implements a 6-stage pipeline:

```
1. Hygiene → 2. Build/Test → 3. Validate → 4. GitHub Release → 5. crates.io → 6. Attest
```

**Stage 1: Hygiene** (from rust.yml)
- Contamination guard verification
- Submodule integrity check
- Clarity contract verification
- Production boundary check
- Gitleaks secret scanning

**Stage 2: Build & Test**
- Cargo build
- Cargo test
- Lightning coverage (>=90%)
- Bitcoin coverage (>=92%)

**Stage 3: Version Validation**
- Verify tag version matches Cargo.toml
- Extract changelog for release notes
- Dry-run `cargo publish`

**Stage 4: GitHub Release** (Automatic)
- Creates release with changelog notes
- Only if release doesn't exist

**Stage 5: crates.io Publish** (Automatic on tag)
- Runs when triggered by git tag push (not workflow_dispatch)
- Requires `CARGO_REGISTRY_TOKEN` in `release` environment
- Must be gated behind all previous checks

**Stage 6: SLSA Attestation**
- Generates build provenance
- Publishes to attestations API

### How to Release

```bash
# 1. Ensure Cargo.toml version matches desired release
# 2. Create and push tag (triggers full pipeline)
git tag v0.4.X
git push origin v0.4.X

# 3. Watch workflow at:
# https://github.com/Conxian/conxian-nexus/actions
```

### Manual Release (workflow_dispatch)

For testing or manual control:
```bash
# Via GitHub CLI
gh workflow run release.yml -f release_version=0.4.X

# Note: workflow_dispatch does NOT trigger crates.io publish
```

## Crate Publishing

### crates.io Configuration

- **Crate Name**: `conxian-nexus`
- **Repository**: Conxian/conxian-nexus
- **Publish Trigger**: Git tag push (automatic)
- **Environment**: Requires `release` environment with `CARGO_REGISTRY_TOKEN`

### Pre-publication Checklist

- [ ] Version in Cargo.toml matches tag
- [ ] CHANGELOG.md has entry for this version
- [ ] All tests pass locally
- [ ] `cargo publish --dry-run` succeeds
- [ ] `CARGO_REGISTRY_TOKEN` is set in GitHub secrets

## GitHub Releases Status

| Version | Git Tag | GitHub Release | crates.io |
|---------|---------|----------------|-----------|
| 0.4.0  | ✅ | ❌ | ❌ |
| 0.4.10 | ✅ | ❌ | ❌ |
| 0.4.17 | ✅ | ✅ | ❌ |
| 0.4.18 | ✅ (created) | ⚡ Pending | ⚡ Pending |
| 0.4.19 | ✅ (created) | ⚡ Pending | ⚡ Pending |

## Related Repositories

| Repo | Purpose | Version |
|------|---------|---------|
| Conxian/conxian-nexus | Glass Node (this repo) | 0.4.19 |
| Conxian/lib-conxian-core | Shared primitives | 0.2.12 |
| Conxian/conxian-gateway | Gateway service | 0.1.4 |
| Conxian/conxius-wallet | Wallet client | - |
| Conxian/conxian_ui | UI components | - |

**Note**: conxian-gateway does NOT depend on conxian-nexus directly. They share lib-conxian-core.

## Security Features (v0.4.19)

- gRPC Authentication
- CORS Middleware
- Rate Limiting (100 concurrent)
- HMAC Constant-time verification
- Safety Mode enforcement
- Redis auth enforcement

## Key Issues to Track

- **#150**: [RELEASE] Publish GitHub release and reconcile version posture
- **#163**: [BIP-110] Verify lighter node sync with limited data
- **#152**: [INFRA] Enable auto-merge for conxian-nexus
- **#151**: [SECURITY] Enforce branch protection

## Session Notes

### 2026-07-15: Release Alignment Session

- Identified missing releases v0.4.18 and v0.4.19
- Created tags and pushed to trigger release workflows
- Enhanced release.yml with full hygiene checks and automatic crates.io publish
- Documentation added for release process
