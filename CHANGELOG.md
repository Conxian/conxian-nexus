# Changelog

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

[Output truncated for brevity]
