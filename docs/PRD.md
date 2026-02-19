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
- **Implementation**: `NexusExecutor` validates transactions against a timestamped cache of on-chain events and detects front-running patterns.

### 2.4 Sovereign Handoff (Safety Monitor)
- **Requirement**: Monitor sync drift between the Nexus and Stacks L1. Trigger a safety mode if the Nexus falls behind.
- **Implementation**: `NexusSafety` heartbeats compare local height with Stacks RPC height. If drift > 2 blocks, "Safety Mode" is triggered, enabling direct withdrawal tenure.

### 2.5 Cryptographic Verification
- **Requirement**: Provide verifiable proofs of state.
- **Implementation**: `NexusState` maintains a Merkle Tree of transaction IDs. REST/gRPC endpoints (`/v1/proof`, `/v1/verify-state`) allow clients to verify data against the state root.

### 2.6 Multi-Protocol Gateway
- **Requirement**: Support multiple protocols including Bisq, RGB, and BitVM.
- **Implementation**: `lib-conxian-core` provides a unified interface (`ConxianService`) for different protocol handlers.

## 3. Technical Stack
- **Language**: Rust (Tokio, Axum, Tonic)
- **Persistence**: PostgreSQL (SQLx), Redis (caching and pub/sub)
- **Cryptography**: Sha256 (Merkle Tree), k256 (ECDSA for wallet)

## 4. Roadmap & Advised Enhancements

### 4.1 Merkle Tree Persistence
- **Issue**: Current Merkle Tree is in-memory and rebuilt on startup.
- **Enhancement**: Implement persistent Merkle Tree storage to handle large datasets efficiently.

### 4.2 Real-time Sync Ingestion
- **Issue**: `NexusSync` currently uses a simulator.
- **Enhancement**: Integrate with Hiro or Stacks node WebSockets for real-time L1 event ingestion.

### 4.3 Advanced MEV Mitigation
- **Issue**: FSOC logic is currently based on simple transaction counts.
- **Enhancement**: Implement mempool monitoring and more sophisticated heuristic analysis for front-running detection.

### 4.4 BitVM Integration
- **Issue**: BitVM is mentioned but not yet implemented in the core gateway.
- **Enhancement**: Fully implement `BitVMService` in `lib-conxian-core`.

### 4.5 Observability
- **Enhancement**: Add Prometheus/OpenTelemetry metrics for tracking sync drift, latency, and service health.
