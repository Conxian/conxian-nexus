# Product Requirement Document: Conxian Nexus (Glass Node)

## 1. Executive Summary
Conxian Nexus is a high-performance middleware designed to bridge off-chain state with Stacks Layer 1 (L1). It serves as a "Glass Node," providing transparency, cryptographic proofs, and enhanced security for decentralized applications and multi-protocol services.

## 2. Core Features & Requirements

### 2.1 Glass Node Architecture
- **Requirement**: Synchronize state with Stacks L1 in real-time.
- **Implementation**: The `nexus-sync` module ingests Stacks node events to maintain an accurate local representation of on-chain data.

### 2.2 Nakamoto Awareness (Epoch 3.0/3.1)
- **Requirement**: Differentiate between microblock soft-finality and burn-block hard-finality.
- **Implementation**: `NexusSync` distinguishes `Microblock` and `BurnBlock` events, updating local state accordingly (`soft` vs `hard`).

### 2.3 FSOC Sequencer (First-Seen-On-Chain)
- **Requirement**: Mitigate MEV (Maximal Extractable Value) by enforcing transaction ordering based on when they were first seen on-chain.
- **Implementation**: `NexusExecutor` validates transactions against a timestamped cache of on-chain events and detects front-running patterns. **Update**: Added sophisticated heuristics for liquidation detection and copy-cat protection.

### 2.4 Sovereign Handoff (Safety Monitor)
- **Requirement**: Monitor sync drift between the Nexus and Stacks L1. Trigger a safety mode if the Nexus falls behind.
- **Implementation**: `NexusSafety` heartbeats compare local height with Stacks RPC height. If drift > 2 blocks, "Safety Mode" is triggered, enabling direct withdrawal tenure.

### 2.5 Cryptographic Verification
- **Requirement**: Provide verifiable proofs of state.
- **Implementation**: `NexusState` maintains a Merkle Tree of transaction IDs. REST/gRPC endpoints (`/v1/proof`, `/v1/verify-state`) allow clients to verify data against the state root. **Update**: Optimized tree calculation and implemented root persistence in Redis.

### 2.6 Multi-Protocol Gateway
- **Requirement**: Support multiple protocols including Bisq, RGB, and BitVM.
- **Implementation**: `lib-conxian-core` provides a unified interface (`ConxianService`) for different protocol handlers. **Update**: Enhanced BitVM support for simulated proof/challenge/verify flows.

## 3. Technical Stack
- **Language**: Rust (Tokio, Axum, Tonic)
- **Persistence**: PostgreSQL (SQLx), Redis (caching and pub/sub)
- **Cryptography**: Sha256 (Merkle Tree), k256 (ECDSA for wallet)

## 4. Roadmap & Advised Enhancements

### 4.1 Persistent Merkle Tree (Full)
- **Status**: Phase 1 Complete (Root persistence).
- **Enhancement**: Implement a full persistent Merkle Tree (e.g., using a Merkle Mountain Range in a key-value store) to handle billions of leaves.

### 4.2 Real-time Sync Ingestion
- **Issue**: `NexusSync` currently uses a simulator.
- **Enhancement**: Integrate with Hiro or Stacks node WebSockets for real-time L1 event ingestion.

### 4.3 Advanced MEV Mitigation (Mempool)
- **Status**: Heuristics Implemented.
- **Enhancement**: Implement mempool monitoring for pre-emptive front-running detection before transactions hit a block.

### 4.4 BitVM Full Lifecycle
- **Status**: Enhanced Stubs.
- **Enhancement**: Integrate with a real BitVM prover/verifier library.

### 4.5 Observability
- **Enhancement**: Add Prometheus/OpenTelemetry metrics for tracking sync drift, latency, and service health.
