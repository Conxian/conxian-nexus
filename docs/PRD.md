# Product Requirement Document: Conxian Nexus (Glass Node)

## 1. Executive Summary
Conxian Nexus is a high-performance middleware designed to bridge off-chain state with Stacks Layer 1 (L1). It serves as a "Glass Node," providing transparency, cryptographic proofs, and enhanced security for decentralized applications and multi-protocol services.

## 2. Core Features & Requirements

### 2.1 Glass Node Architecture
- **Requirement**: Synchronize state with Stacks L1 in real-time.
- **Implementation**: The `nexus-sync` module maintains a local representation of on-chain data using an asynchronous channel-based ingestion loop.

### 2.2 Nakamoto Awareness (Epoch 3.0/3.1)
- **Requirement**: Differentiate between microblock soft-finality and burn-block hard-finality.
- **Implementation**: `NexusSync` distinguishes `Microblock` and `BurnBlock` events, updating local state accordingly.

### 2.3 FSOC Sequencer (First-Seen-On-Chain)
- **Requirement**: Mitigate MEV by enforcing transaction ordering based on when they were first seen on-chain.
- **Implementation**: `NexusExecutor` validates transactions against a timestamped cache. Enhanced with Sandwich Attack detection and liquidation front-running heuristics.

### 2.4 Sovereign Handoff (Safety Monitor)
- **Requirement**: Monitor sync drift between the Nexus and Stacks L1.
- `NexusSafety` heartbeats compare local height with Stacks RPC height. If drift > 2 blocks, "Safety Mode" is triggered.

### 2.5 Cryptographic Verification
- **Requirement**: Provide verifiable proofs of state and persist state roots.
- **Implementation**: `NexusState` maintains a Merkle Tree of transaction IDs with intermediate level caching. **New**: Added MMR (Merkle Mountain Range) foundation for future persistent audit logs.

### 2.6 Multi-Protocol Gateway
- **Requirement**: Support multiple protocols including Bisq, RGB, and BitVM.
- **Implementation**: `lib-conxian-core` provides a unified interface (`ConxianService`). **Updated**: Services now return structured `ServiceResponse` with rich metadata and data payloads.

### 2.7 Wallet & Security
- **Requirement**: Secure signing and key management.
- **Implementation**: **New**: `lib-conxian-core` Wallet now supports BIP-39 mnemonics for secure key backup and recovery.

### 2.8 B2B License & Billing Enforcement (Sovereign Grace Period)
- **Requirement**: Prevent hard-failures for B2B SDK clients when limits are exceeded.
- **Implementation**: Billing module implements a 24-hour "Sovereign Grace Period" with 40% efficiency after 50k signatures.

## 3. Technical Stack
- **Language**: Rust (Tokio, Axum, Tonic)
- **Persistence**: PostgreSQL (SQLx), Redis (caching and pub/sub)
- **Cryptography**: Sha256 (Merkle Tree/MMR), k256 (ECDSA), BIP-39 (Mnemonic)
- **Observability**: Prometheus, OpenTelemetry (Tracing)

## 4. Roadmap & Status

### 4.1 Persistent Merkle Tree & MMR (Full)
- **Status**: Merkle Tree complete; MMR Foundation implemented.
- **Next Step**: Implement a full persistent Merkle Mountain Range (MMR) in a key-value store for historical state auditing.

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
- **Status**: Internal Metrics, Prometheus Exporter, and OTel Instrumentation Complete.
- **Next Step**: Add full OpenTelemetry collector integration for distributed tracing.
