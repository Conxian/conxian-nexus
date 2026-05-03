# Changelog

All notable changes to this project will be documented in this file.

## [0.4.3] - 2024-05-27

### Added
- **Hardened Repository Hygiene**: Updated `.gitignore` and `.dockerignore` to exclude standard artifacts (`node_modules`, `test-results`, `playwright-report`) as per Conxian baseline.
- **CI Guardrail Refactor**: Refactored `scripts/check_production_boundary.sh` to use separate `EXCLUDE_DIRS` and `EXCLUDE_FILES` arrays, improving maintainability and ensuring full coverage of production paths.

### Changed
- **Consolidated Governance**: Merged root `CODEOWNERS` into `.github/CODEOWNERS` and standardized on team-based ownership with explicit fallback maintainers.

## [0.4.2] - 2024-05-26

### Added
- **Full API Specification Sync**: Synchronized `docs/openapi.yaml` with all current modules including Analytics, ZKML, ERP Sync, Identity Resolution, and Bitcoin DLC Bonds.
- **Enhanced MMR Logic**: Consolidated helper functions in `src/state/mod.rs` for O(log N) sibling path calculation and resolved all dead code warnings.

### Fixed
- **Kwil Persistence Alignment**: Updated `KwilAdapter` to include mandatory `created_at` timestamps in block and state-root commitments, aligning with the `nexus_pilot` schema.
- **State Proof Robustness**: Corrected MMR metadata calculation logic to properly handle right-child sibling resolution during inclusion proof generation.

## [0.4.1] - 2024-05-25

### Added
- **Kwil Sovereign SQL Pilot**: Implemented the `KwilAdapter` for decentralized relational state persistence, enabling block and state-root commitments to Kwil.
- **Enhanced Mainnet Readiness**: Corrected test fixtures and `app_router` signatures to align with current production architecture.
- **Competitive Research Summary**: Documented findings in `docs/enhancements_summary.md` for future autonomous BOS capabilities.

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

## [0.4.4] - 2024-05-28

### Added
- **Enhanced Oracle Aggregation**: Implemented confidence interval weights and outlier rejection (10% threshold) for multi-source FX rate aggregation in `OracleAggregator`.
- **Institutional Signal Verification**: Integrated cross-verification of external settlement signals (ISO 20022) against aggregated oracle rates with a 5% tolerance in `OracleService`.

### Changed
- **Updated PRD**: Synchronized `docs/PRD.md` with the latest functional enhancements and versioned to v0.4.4.

## [0.4.5] - 2024-05-29

### Added
- **MMR Persistence in Kwil (Phase 2)**: Extended `KwilAdapter` and `NexusSync` to mirror cryptographic MMR nodes to the sovereign Kwil layer, ensuring a decentralized audit trail for state reconstruction.
- **Enhanced Kwil Error Handling**: Implemented explicit warning logs for sovereign persistence failures to maintain sync loop availability while providing visibility into infrastructure health.

### Fixed
- **Local Branch Hygiene**: Automated cleanup of merged branches during the sync cycle.

## [0.4.6] - 2024-05-30

### Fixed
- **Oracle Persistence Logic**: Corrected a critical SQL syntax error in `OracleService` where placeholders and columns were misaligned during FX rate persistence.

### Changed
- **Repository Hygiene**: Removed legacy automation scripts (`update_main_v3.py`) from the repository root.
