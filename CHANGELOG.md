# Changelog

All notable changes to this project will be documented in this file.

## [0.4.0] - 2024-05-24

### Added
- **Full MMR Inclusion Proofs**: Optimized the logic for calculating MMR sibling positions to $O(\log N)$ and ensured integration with the `/v1/mmr-proof` API endpoint.

## [0.3.0] - 2024-05-23

### Added
- **High-Performance Merkle Tree**: Implemented intermediate level caching in `NexusState` for O(logN) proof generation and optimized root calculation.
- **Enhanced FSOC Sequencer**: Added "Sandwich Attack" detection and refined liquidation front-running heuristics (200ms window).
- **Asynchronous Sync Ingestion**: Refactored `NexusSync` to use a channel-based event loop, decoupling polling from processing.
- **Fast-Path Ingestion**: Added `fast_path_ingest` to `NexusSync` for real-time microblock updates via external triggers.
- **Multi-Protocol Enhancements**:
    - BitVM: Added state transition root simulation.
    - RGB: Added schema-specific validation (LNPBP, NIA) and cryptographic state proof verification.

### Fixed
- Improved Merkle Tree logic to handle odd leaf counts and empty trees more robustly.
- Corrected SQL parameter bindings in `NexusExecutor` MEV detection queries.

## [0.2.0] - 2024-05-22

### Added
- Prometheus metrics exporter at `/metrics`.
- Historical state root persistence in PostgreSQL (`nexus_state_roots` table).
- Enhanced simulation logic for BitVM (multi-step proof generation) and RGB (schema validation) services.
- Tracking of sync drift and safety mode status in Prometheus metrics.

### Fixed
- Corrected SQL placeholders in `nexus-sync` module (fixed empty `` placeholders to `$1`, `$2`, etc.).
- Robust state rebuilding in `load_initial_state` to correctly handle database results.

### Security
- FSOC Sequencer now increments transaction metrics upon successful validation.

## [0.5.0] - 2026-04-14

### Added
- **Nostr Telemetry Collector**: Implemented `NostrCollector` to subscribe to telemetry events (Kind 26001) and bridge usage metrics to Redis (CON-473).
- **Fail-Closed Custody Controls**: Hardened `NexusExecutor` and settlement handlers with fail-closed logic when in Safety Mode or when control dependencies are unavailable (CON-460).
- **Sovereign Relational Persistence**: Integrated state-root anchoring to Tableland for decentralized relational state commitment (CON-69).

### Improved
- **Production Boundary Security**: Validated repository hygiene against testnet contamination using standard CI guardrails (CON-411).
- **Sync Integrity**: Hardened `NexusSync` with microblock reorg detection and automated state reconstruction from hard-finality tips (NEXUS-03).
- **Dependency Consolidation**: Standardized on remote git dependency for `lib-conxian-core` to eliminate local drift (CON-67).

### Fixed
- **MMR Proof Endpoint**: Corrected SQL parameter binding in `/v1/mmr-proof` query.
