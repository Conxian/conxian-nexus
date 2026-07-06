# Changelog

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
