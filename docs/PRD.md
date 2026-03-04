# Product Requirement Document: Conxian Nexus (Glass Node)

## 1. Executive Summary
Conxian Nexus is a high-performance middleware designed to bridge off-chain state with Stacks Layer 1 (L1). It serves as a "Glass Node," providing transparency, cryptographic proofs, and enhanced security for decentralized applications and multi-protocol services.

## 2. Core Features & Requirements

### 2.1 Glass Node Architecture
- **Requirement**: Synchronize state with Stacks L1 in real-time.
- **Implementation**: The `nexus-sync` module maintains a local representation of on-chain data. **Update**: Transitioned to an asynchronous channel-based ingestion loop for improved throughput and lower latency.

### 2.2 Nakamoto Awareness (Epoch 3.0/3.1)
- **Requirement**: Differentiate between microblock soft-finality and burn-block hard-finality.
- **Implementation**: `NexusSync` distinguishes `Microblock` and `BurnBlock` events, updating local state accordingly (`soft` vs `hard`).

### 2.3 FSOC Sequencer (First-Seen-On-Chain)
- **Requirement**: Mitigate MEV (Maximal Extractable Value) by enforcing transaction ordering based on when they were first seen on-chain.
- **Implementation**: `NexusExecutor` validates transactions against a timestamped cache of on-chain events. **Update**: Enhanced with Sandwich Attack detection and 200ms liquidation front-running heuristics.

### 2.4 Sovereign Handoff (Safety Monitor)
- **Requirement**: Monitor sync drift between the Nexus and Stacks L1. Trigger a safety mode if the Nexus falls behind.
- **Implementation**: `NexusSafety` heartbeats compare local height with Stacks RPC height. If drift > 2 blocks, "Safety Mode" is triggered, enabling direct withdrawal tenure.

### 2.5 Cryptographic Verification
- **Requirement**: Provide verifiable proofs of state and persist state roots.
- **Implementation**: `NexusState` maintains a Merkle Tree of transaction IDs. **Update**: Optimized with intermediate level caching for O(logN) proof generation. State roots are persisted in PostgreSQL.

### 2.6 Multi-Protocol Gateway
- **Requirement**: Support multiple protocols including Bisq, RGB, and BitVM.
- **Implementation**: `lib-conxian-core` provides a unified interface (`ConxianService`). **Update**: Enhanced BitVM with state transition simulation and RGB with schema-specific validation (LNPBP/NIA).

### 2.7 B2B License & Billing Enforcement (Sovereign Grace Period)
- **Requirement**: Prevent hard-failures for B2B SDK clients when limits are exceeded.
- **Implementation**: Billing module implements a 24-hour "Sovereign Grace Period" with 40% efficiency (randomized 60% drop rate) after 50k signatures.

## 3. Technical Stack
- **Language**: Rust (Tokio, Axum, Tonic)
- **Persistence**: PostgreSQL (SQLx), Redis (caching and pub/sub)
- **Cryptography**: Sha256 (Merkle Tree), k256 (ECDSA for wallet)
- **Observability**: Prometheus, Tracing

## 4. Roadmap & Status

### 4.1 Persistent Merkle Tree (Full)
- **Status**: Phase 3 Complete (High-performance tree with intermediate caching).
- **Next Step**: Implement a full persistent Merkle Mountain Range (MMR) in a key-value store.

### 4.2 Real-time Sync Ingestion
- **Status**: Channel-based Async Ingestion Complete.
- **Next Step**: Integrate with Hiro or Stacks node WebSockets using the `fast_path_ingest` endpoint.

### 4.3 Advanced MEV Mitigation (Mempool)
- **Status**: FSOC Sequencer with Sandwich Detection Complete.
- **Next Step**: Implement full mempool monitoring for pre-emptive front-running detection.

### 4.4 BitVM Full Lifecycle
- **Status**: State Transition Simulation Complete.
- **Next Step**: Integrate with a real BitVM prover/verifier library.

### 4.5 Observability
- **Status**: Internal Metrics and Prometheus Exporter Complete.
- **Next Step**: Add OpenTelemetry tracing and structured logging.
