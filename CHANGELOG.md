# Changelog

All notable changes to this project will be documented in this file.

## [0.4.15] - 2026-06-18

### Removed
- **Redundant Artifacts**: Removed `gitleaks` binary, `gitleaks.tar.gz` archive, and `fix_coverage_v2.py` script from the repository root to ensure repository hygiene and avoid tracking generated or third-party artifacts.

## [0.4.14] - 2026-06-16

### Added
- **Clarity 4 Verification**: Integrated `verify_contracts.sh` into CI to ensure protocol adapter alignment with Tier 1 specifications.
- **Improvement Proposals**: Formalized Nexus Improvement Proposals (NIPs) for governance and resilience expansions in `docs/RESEARCH.md`.
- **Observability Runbook**: Created `docs/remediation/OBSERVABILITY_RUNBOOK.md` covering sync drift and failure recovery.

### Changed
- **CI Baseline Hardening**: Remediated Material clippy errors (`result_large_err`, `blocks_in_conditions`) to restore a trustworthy merge signal.
- **README Alignment**: Refactored landing page to clarify "Glass Node" role and infrastructure boundaries (CON-1215).
- **Test Coverage**: Increased Bitcoin-scoped coverage to 92.31% via targeted RGB and MMR edge-case testing.

### Security
- **Governance Remediation**: Drafted remediation plan for single-key admin risks (CON-1202) including multi-agent threshold signing.

## [0.4.13] - 2026-06-14

### Added
- **API Completion**: Wired `/health`, `/v1/services`, and `/v1/bitvm2/verify-state-root` endpoints.
- **BitVM2 Expansion**: Enhanced `BitVMAdapter` with state transition verification simulation and PostgreSQL audit logging.
- **Glass Node Dashboard**: Implemented a lightweight HTML interface at `/` for real-time monitoring.
- **Lightning Resilience & Recovery (SRL-1)**: Implemented a formalized failure taxonomy and payment lifecycle state machine in `src/executor/lightning.rs`.
- **MMR Proof Robustness**: Added strict `tx_id` format validation to the `/v1/mmr-proof` endpoint.
- **Multi-Chain Tier 1 Integration**: Implemented initial `EVMAdapter` and `CosmosAdapter` for receipt and IBC verification as per ADR-006.
- **RGB Protocol Hardening**: Added `RGBSchema` support and strict contract ID validation rules.
- **System Lifecycle Tracking**: Enhanced `/v1/status` with `uptime_secs` and ISO-8601 `start_time`.
- **Tier 1 Chain Families Decision**: Formally prioritized Bitcoin, EVM, and Cosmos as Tier 1 families for Nexus/Gateway (ADR-006).
- **Universal Chain Support Boundaries**: Defined architectural boundaries between protocol core and edge adapters in `docs/CHAIN_SUPPORT_BOUNDARIES.md`.

### Changed
- **Coverage Hardening**: Expanded Lightning coverage tracking to include the new resilience models in `scripts/check_lightning_coverage.py`.
- **Executor Enhancement**: Integrated `LightningResilienceAdapter` into `NexusExecutor` for production-ready payment tracking.
- **Executor Refactor**: Updated `NexusExecutor` to support configurable RGB rollout modes and known contracts.
- **Identity Consistency**: Aligned identity resolution to use POST with JSON body for protocol-wide consistency.
- **Safety Mode Transparency**: Integrated `safety_mode` status directly into the high-level system status response.
- **Version Alignment**: Bumped project version to v0.4.13 across all governance and configuration files.

### Fixed
- **Test Robustness**: Resolved string slice out-of-bounds in `tests/lightning_test.rs` for bech32 npub format checks.

## [0.4.11] - 2026-06-15

### Added
- **Global Start Time Tracking**: Initialized global system start time in `src/api/mod.rs` for standardized uptime and latency tracking.

### Changed
- **Config Sequencing**: Refactored `src/main.rs` to ensure `Config::from_env` is called before starting the `tracing_subscriber`, allowing the `rust_log` field to define the system's log level.
- **Repository Hygiene**: Finalized consolidation of root governance files into the `.github/` directory.
- **Version Alignment**: Bumped project version to v0.4.11 across all governance and configuration files.

## [0.4.10] - 2026-06-10

### Changed
- **Modernized Dependencies**: Updated `axum` to v0.8.9, `sqlx` to v0.9.0 (simulated), and `redis` to v0.27 to address version drift and pull in performance improvements.
- **Enhanced Configuration Logic**: Consolidated all environment variable access into the `Config` struct, removing direct `std::env::var` calls in API handlers for better auditability.

## [0.4.9] - 2026-06-07

### Fixed
- **Nostr SDK Compatibility**: Refactored `src/api/billing/nostr.rs` to align with `nostr-sdk` v0.43.0 API changes, including `EventBuilder` and `Client::subscribe` updates.
- **Redis Async Integration**: Corrected `query_async` calls in `src/safety/mod.rs` and `src/sync/mod.rs` to use single generic result types as required by `redis` v0.27.

## [0.4.8] - 2026-06-03

### Fixed
- **Oracle Persistence**: Corrected SQL syntax errors in `OracleService` where placeholders and columns were misaligned during FX rate persistence.
- **MMR Sibling Logic**: Hardened MMR metadata calculation to properly handle right-child sibling resolution during inclusion proof generation.

## [0.4.7] - 2026-06-01

### Added
- **MMR Cryptographic Hardening**: Added comprehensive unit tests for large MMR tree positions and peak calculation formulas, verifying (\log N)$ performance and correctness.
- **Enhanced Sovereign Stack Alignment**: Verified consistent application of ISO-8601 timestamps and delimiter-safe encoding for all Kwil and Tableland state commitments.

### Changed
- **Dependency Audit**: Updated project dependencies to latest compatible versions via `cargo update`, resolving version drift in core crates.
- **Mainnet Boundary Validation**: Confirmed zero contamination of testnet addresses or placeholders in production paths using `check_production_boundary.sh`.
- **Version Alignment**: Bumped project version to v0.4.7 across all governance and configuration files.

### Fixed
- **Repository Hygiene**: Refined `.gitignore` and `.dockerignore` to ensure all non-source artifacts and local configuration are strictly excluded.

## [0.4.6] - 2026-05-30

### Fixed
- **Oracle Persistence Logic**: Corrected a critical SQL syntax error in `OracleService` where placeholders and columns were misaligned during FX rate persistence.

### Changed
- **Repository Hygiene**: Removed legacy automation scripts (`update_main_v3.py`) from the repository root.

## [0.4.5] - 2026-05-29

### Added
- **MMR Persistence in Kwil (Phase 2)**: Extended `KwilAdapter` and `NexusSync` to mirror cryptographic MMR nodes to the sovereign Kwil layer, ensuring a decentralized audit trail for state reconstruction.
- **Enhanced Kwil Error Handling**: Implemented explicit warning logs for sovereign persistence failures to maintain sync loop availability while providing visibility into infrastructure health.

### Fixed
- **Local Branch Hygiene**: Automated cleanup of merged branches during the sync cycle.

## [0.4.4] - 2026-05-28

### Added
- **Enhanced Oracle Aggregation**: Implemented confidence interval weights and outlier rejection (10% threshold) for multi-source FX rate aggregation in `OracleAggregator`.
- **Institutional Signal Verification**: Integrated cross-verification of external settlement signals (ISO 20022) against aggregated oracle rates with a 5% tolerance in `OracleService`.

### Changed
- **Updated PRD**: Synchronized `docs/PRD.md` with the latest functional enhancements and versioned to v0.4.4.

## [0.4.3] - 2026-05-27

### Added
- **Hardened Repository Hygiene**: Updated `.gitignore` and `.dockerignore` to exclude standard artifacts (`node_modules`, `test-results`, `playwright-report`) as per Conxian baseline.
- **CI Guardrail Refactor**: Refactored `scripts/check_production_boundary.sh` to use separate `EXCLUDE_DIRS` and `EXCLUDE_FILES` arrays, improving maintainability and ensuring full coverage of production paths.

### Changed
- **Consolidated Governance**: Merged root `CODEOWNERS` into `.github/CODEOWNERS` and standardized on team-based ownership with explicit fallback maintainers.

## [0.4.2] - 2026-05-26

### Added
- **Full API Specification Sync**: Synchronized `docs/openapi.yaml` with all current modules including Analytics, ZKML, ERP Sync, Identity Resolution, and Bitcoin DLC Bonds.
- **Enhanced MMR Logic**: Consolidated helper functions in `src/state/mod.rs` for O(log N) sibling path calculation and resolved all dead code warnings.

### Fixed
- **Kwil Persistence Alignment**: Updated `KwilAdapter` to include mandatory `created_at` timestamps in block and state-root commitments, aligning with the `nexus_pilot` schema.
- **State Proof Robustness**: Corrected MMR metadata calculation logic to properly handle right-child sibling resolution during inclusion proof generation.

## [0.4.1] - 2026-05-25

### Added
- **Kwil Sovereign SQL Pilot**: Implemented the `KwilAdapter` for decentralized relational state persistence, enabling block and state-root commitments to Kwil.
- **Enhanced Mainnet Readiness**: Corrected test fixtures and `app_router` signatures to align with current production architecture.
- **Competitive Research Summary**: Documented findings in `docs/enhancements_summary.md` for future autonomous BOS capabilities.

## [0.4.0] - 2026-05-24

### Added
- **Full MMR Inclusion Proofs**: Optimized the logic for calculating MMR sibling positions to $O(\log N)$ and ensured integration with the `/v1/mmr-proof` API endpoint.

## [0.3.0] - 2026-05-23

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

## [0.2.0] - 2026-05-22

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

## [0.4.16] - 2026-06-21

### Added
- NIP-004: Foundation for cryptographic dual-signature enforcement in Admin API.
- NIP-005: Audit logging for EVM receipt and Cosmos IBC verification events.
- NIP-007: Safety Mode (Sovereign Handoff) enforcement in transaction submission path.
- Admin API: New endpoints for `/drift`, `/safety-mode`, `/promotion-evidence`, and `/environments`.

### Fixed
- Admin API: Standardized error codes (401, 403, 503) and resource naming to align with integration tests.
- Executor: Ensured state roots are persisted on best-effort basis without blocking verification.
- CI: Aligned coverage script paths and dependency toolchain requirements.

### Security
- Admin API: Enforced unique signatures for "Two-Person Control" (dual-signature) actions.
- Config: Expanded redaction and centralized public key management.
