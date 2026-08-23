# Changelog

## [0.4.23] - 2026-08-18

### Added
- **Hole 2.1 Hardware Enclave X.509 DER Certificate Verification**: Upgraded `src/executor/mod.rs` to parse X.509 DER attestation certificates using `x509-cert`, enforce validity window bounds (`not_before` / `not_after`), and reject invalid or expired attestation envelopes.
- **CON-1304 Fedimint Phase 2 Cryptographic Audit**: Implemented Fedimint e-cash blinded mint proof verification in `src/executor/fedimint.rs`, including prefix checks (`fed:`, `fed1:`), payload length validation, SHA-256 nonce hash derivation, double-spend detection against PostgreSQL, and audit persistence (`migrations/20260818000000_fedimint_mint_audit.sql`).
- **NIP-005 Phase 2 Cryptographic Verification**: Upgraded EVM (MPT receipt proof Keccak-256 root matching) and Cosmos (IBC Tendermint base64 header SHA-256 digest computation) adapters to full cryptographic verification.
- **CON-24 B2B Paid Subscription Tiers**: Implemented `Free`, `Pro`, and `Enterprise` subscription tiers with feature-gating for DLC, ZKML, Tableland, and canonical BitVM (`src/api/billing/mod.rs`).
- **Lightning Network Tier Upgrades**: Added `/billing/upgrade` and `/billing/verify-payment` REST endpoints to facilitate automated tier upgrades settled via Lightning Network invoices.
- **CON-1533 BitVM Groth16 Research Salvage**: Salvaged BitVM Groth16 research artifacts into unit test coverage and verifier integration.

### Changed
- Synchronized repository documentation (`docs/GAP_ANALYSIS.md`, `docs/RESEARCH.md`, and `CHANGELOG.md`) with v0.4.23 implementation state.

## [0.4.22] - 2026-07-15

### Changed
- Updated release workflow: stages now run independently for better reliability

## [0.4.21] - 2026-07-15

### Changed
- Enhanced AGENTS.md with comprehensive release documentation
- Added crates.io setup requirements to documentation
- Updated pre-publish checklist

### Fixed
- Release workflow: publish and attest now independent of create-github-release stage

## [0.4.20] - 2026-07-15

### Changed
- Enhanced AGENTS.md with version alignment matrix and release process documentation
- Updated AGENTS.md with multi-dimensional knowledge base structure

## [0.4.19] - 2026-07-07

### Added
- **NIP-005 Hardening**: Enhanced structural validation for EVM, Cosmos, Fedimint, and Stacks adapters.
- **API Refactoring**: Implemented `AppConfig` for REST server initialization to improve state isolation and testability.
- **Route Synchronization**: Integrated missing DLC, Identity, and Services routes into the primary router.

### Changed
- Refactored `src/api/rest.rs` and `src/main.rs` to use centralized `AppConfig`.
- Updated test suites for all protocol adapters to verify hardened structural checks.

### Fixed
- Removed unused code and dead variants in `admin.rs` and `erp.rs` to resolve compiler warnings.
- Corrected type inference issues in `sqlx` and `reqwest` call sites across the API layer.

## [0.4.18] - 2026-07-06

### Added
- **NIP-006**: Scoped Admin API Keys and Dual-Signature Login (`/admin/v1/login`).
- **Hole 1.2**: Hardened Redis connection enforcement (authentication required in release builds).
- **Hole 3.1**: SRL-1 Resilience recovery triggers (Retry, Split-Recovery, Reconciliation).

### Changed
- Refactored `src/api/admin.rs` to prioritize scoped credentials over static fallback token.
- Updated `Storage::new` to bail on unauthenticated Redis in release builds unless overridden.
- Synchronized `docs/GAP_ANALYSIS.md` and `docs/RESEARCH.md` with v0.4.18 implementations.

## [0.4.17] - 2026-06-27

### Added
- **NIP-005**: Real Groth16 cryptographic verification for BitVM2 transitions using `ark-groth16`.
- **Hole 4.1**: Expanded MEV audit logging with full transaction payloads and sequencer priority metadata.
- **NIP-004**: Cryptographic dual-signature verification for release approvals and governance.
- **NIP-007**: Safety Mode enforcement in the submission path.
