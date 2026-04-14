# Product Requirement Document: Conxian Nexus (Glass Node)

## 1. Executive Summary
Conxian Nexus is a high-performance middleware designed to bridge off-chain state with Stacks Layer 1 (L1). It serves as a "Glass Node," providing transparency, cryptographic proofs, and enhanced security for decentralized applications and multi-protocol services.

## 2. Core Features & Requirements

### 2.1 Glass Node Architecture
- **Requirement**: Synchronize state with Stacks L1 in real-time.
- **Implementation**: The `nexus-sync` module maintains a local representation of on-chain data using an asynchronous channel-based ingestion loop. **Final (v0.4.0)**: Integrated Microblock Reorg Detection with automated rollback and persistent MMR peaks/leaves.

### 2.2 Nakamoto Awareness (Epoch 3.0/3.1)
- **Requirement**: Differentiate between microblock soft-finality and burn-block hard-finality.
- **Implementation**: `NexusSync` distinguishes `Microblock` and `BurnBlock` events, updating local state accordingly.

### 2.3 FSOC Sequencer (First-Seen-On-Chain)
- **Requirement**: Mitigate MEV by enforcing transaction ordering based on when they were first seen on-chain.
- **Implementation**: `NexusExecutor` validates transactions against a timestamped cache. **New (v0.4.0)**: Integrated **MEV Transparency Logging** for on-chain auditability of rejected transactions.

### 2.4 Sovereign Handoff (Safety Monitor)
- **Requirement**: Monitor sync drift between the Nexus and Stacks L1.
- `NexusSafety` heartbeats compare local height with Stacks RPC height. If drift > 2 blocks, "Safety Mode" is triggered.

### 2.5 Cryptographic Verification
- **Requirement**: Provide verifiable proofs of state and persist state roots.
- **Implementation**: `NexusState` maintains a Merkle Tree of transaction IDs. **Superior (v0.4.0)**: Implemented Persistent Merkle Mountain Range (MMR) peaks AND full leaf persistence in PostgreSQL (`mmr_nodes`) for O(1) audit log restoration.

### 2.6 Multi-Protocol Gateway
- **Requirement**: Support multiple protocols including Bisq, RGB, and BitVM.
- **Implementation**: `lib-conxian-core` provides a unified interface (`ConxianService`). **New (v0.4.0)**: Integrated **ContractBridge** for signed Clarity contract calls to Stacks.

### 2.7 Wallet & Security
- **Requirement**: Secure signing and key management.
- **Implementation**: `lib-conxian-core` Wallet supports BIP-39 mnemonics and BIP-32 HD derivation.

### 2.8 B2B License & Billing Enforcement (Sovereign Grace Period)
- **Requirement**: Prevent hard-failures for B2B SDK clients when limits are exceeded.
- **Implementation**: Billing module implements a 24-hour "Sovereign Grace Period" with 40% efficiency. **Secure (v0.4.0)**: Telemetry reporting utilizes HMAC-SHA256 verification. **Decentralized (v0.5.0)**: Integrated **Nostr Telemetry Integrated **Nostr Telemetry Bridge** for signed, decentralized usage reporting. Health Bridge** for signed, decentralized reporting (v0.5.0).

### 2.9 Sovereign Transactional SQL (Pilot)
- **Requirement**: Evaluate and pilot Kwil and Tableland as sovereign OLTP and commitment layers to replace hosted PostgreSQL for critical state.
- **Implementation**: **New (v0.5.0)**: Implemented `KwilAdapter` and `TablelandAdapter` for decentralized relational state persistence. Designed pilot schema (`docs/kwil_pilot_schema.sql`) for block and state-root anchoring.

## 3. Technical Stack
- **Language**: Rust (Tokio, Axum, Tonic)
- **Persistence**: PostgreSQL (SQLx), Redis (caching and pub/sub), **Kwil & Tableland (Sovereign Pilot)**
- **Decentralized Comms**: **Nostr (Telemetry & Agentic Coordination)**
- **Cryptography**: Sha256 (Merkle Tree/MMR/HMAC), k256 (ECDSA), BIP-39 (Mnemonic), BIP-340 (Schnorr/Nostr)
- **Observability**: Prometheus, OpenTelemetry (Tracing), **MEV Audit Logs**, **Nostr Telemetry**

## 4. Roadmap & Status

### 4.1 Persistent Merkle Tree & MMR (Complete)
- **Status**: Merkle Tree complete; **Persistent MMR Peaks, Leaf Nodes, and Full Inclusion Proofs implemented (v0.4.0)**. **Real-time Polling implemented (v0.5.0)**.
- **Next Step**: Implement full MMR audit trail visualization.

### 4.2 Real-time Sync Ingestion
- **Status**: Channel-based Async Ingestion Complete; Reorg detection with automated rollback implemented.
- **Next Step**: Integrate with Hiro or Stacks node WebSockets.

### 4.3 Advanced MEV Mitigation (Mempool)
- **Status**: FSOC Sequencer with Sandwich Detection and **Transparency Logging** Complete.
- **Next Step**: Implement full mempool monitoring.

### 4.4 BitVM Full Lifecycle
- **Status**: State Transition Simulation Complete.
- **Next Step**: Integrate with a real BitVM prover/verifier library.

### 4.5 Oracle & Rebalancing
- **Status**: **Historical FX Persistence**, **Dynamic LTV Rebalancing**, and **Multi-source Aggregated Oracle** Complete (v0.4.0).
- **Next Step**: Implement confidence interval weights for oracle sources.

### 4.6 Sovereign Infrastructure Migration (In Progress)
- **Status**: **Kwil and Tableland Pilot Implementations Complete (v0.5.0)**. **Nostr Health Reporting implemented (v0.5.0)**.
- **Next Step**: Full migration of transactional state to Kwil/Sovereign SQL and telemetry to Nostr.

## 5. Mainnet Readiness Evidence Pack (v0.5.0)

### 5.1 Security & TEE (CON-162)
- **External Triggers**: ISO 20022, PAPSS, and BRICS triggers are now wired into the execution flow.
- **Verification Logic**: All external signals require valid TEE attestation and Oracle cross-verification before emitting a state proposal.
- **Time-locks**: Verified triggers initiate a mandatory 144-block time-lock in the `settlement_proposals` table, preventing direct contract execution from TradFi payloads.

### 5.2 Hygiene & Contamination (CON-394/183)
- **Branch Model**: Production branches are mainnet-only. All testnet defaults (ST... addresses) have been removed from critical paths.
- **Secrets Management**: No secrets tracked in source control. Standardized `.env.example` provided with security guidelines.
- **Artifact Control**: `.gitignore` and `.dockerignore` hardened to exclude build artifacts and local configuration.

### 5.3 Signer & Wallet Governance (CON-229)
- **Bootstrap Wallet**: Aligned with the canonical SAB-owned wallet (`SPSZXAKV7DWTDZN2601WR31BM51BD3YTQWE97VRM`) for identity resolution and signing operations.
- **Signer Controls**: `lib-conxian-core` handles HD derivation and secure message signing, allowing for multi-sig transitions and DAO handoff.

### 5.4 Rollback & Finality (NEXUS-03)
- **Microblock Reorgs**: NexusSync detects microblock reorgs and performs automated state rollbacks to the last hard-finality (burn block) tip.
- **MMR Persistence**: Persistent MMR peaks and nodes ensure state roots can be reconstructed from the on-chain audit trail with O(1) performance.

### 5.5 Sovereign Persistence & Telemetry (CON-69, CON-473)
- **Tableland**: RELATIONAL state commitments are bridged to decentralized Tableland tables.
- **Nostr**: Signed telemetry reporting removes centralized ingest bottlenecks and enhances agentic sovereignty.
