# Changelog

All notable changes to this project will be documented in this file.

## [0.4.0] - 2026-03-17

### Added
- **Full MMR Persistence**: Implemented `mmr_nodes` table in PostgreSQL to store all Merkle Mountain Range nodes, enabling exhaustive cryptographic audit trails.
- **Automated Reorg Rollback**: Enhanced `NexusSync` with a robust rollback mechanism for microblock reorgs. It now marks orphaned blocks and reconstructs state from the last valid hard-finality tip.
- **Multi-Source Aggregated Oracle**: Upgraded `OracleStub` to fetch FX rates from multiple providers and apply median-based aggregation for increased reliability.
- **State Consistency Fixes**: Updated API endpoints (`/v1/status`, `/v1/metrics`) and state loading to strictly ignore orphaned block data.

### Fixed
- Resolved multiple Clippy warnings including needless borrows, useless formats, and manual `div_ceil` reimplementations.
- Updated `build.rs` to use the modern `compile_protos` method instead of the deprecated `compile`.

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
