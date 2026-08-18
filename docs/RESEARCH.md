# Conxian Nexus Research & Improvement Proposals (Updated August 2026 - v0.4.23)

## 1. Multi-Chain Interoperability (NIP-005)

### 1.1 Bitcoin & BitVM2
- **Concept**: Optimistic bridge research for trust-minimized Bitcoin L2s and state transition verification.
- **Status**: The canonical BN254 boundary uses `ark-groth16` in `bitvm_groth16`, composed by `canonical_bitvm` for trusted-key lookup, height validation, audit persistence, and HTTP handling. Real cryptographic verification of Groth16 SNARK proofs against verified verifying keys is active.

### 1.2 Cosmos & IBC Header Verification
- **Concept**: Trust-minimized cross-chain state proofs using the Inter-Blockchain Communication (IBC) protocol and Tendermint header validation.
- **Implementation Path**: Decodes base64 Tendermint client update headers, computes SHA-256 header payload digests, and enforces strict block height progression (`latest_height > trusted_height`) with persistent audit logging in `cosmos_verified_client_updates`.
- **Status**: **Upgraded to Cryptographic Verification (v0.4.23)** in `src/executor/cosmos.rs`.

### 1.3 EVM Merkle Patricia Trie (MPT) Receipt Proof Verification
- **Concept**: Verifying that a transaction receipt belongs to a specific block's receipt root via Merkle Patricia Trie (MPT) node hash chain verification.
- **Implementation Path**: Parses hex-encoded proof nodes and receipt roots, verifies that node 0 Keccak-256 hash equals `receipt_root`, verifies parent-child hash linkages across the branch, and persists audit state in `evm_verified_receipts`.
- **Status**: **Upgraded to Cryptographic Verification (v0.4.23)** in `src/executor/evm.rs`.

## 2. Admin & Governance Hardening

### 2.1 Cryptographic Dual-Signatures (NIP-004)
- **Status**: **COMPLETED v0.4.17**. Secp256k1 signature verification active for all write/governance endpoints using `k256`.

### 2.2 Admin Token Hardening (NIP-006)
- **Status**: **COMPLETED v0.4.18**. Replaced static bearer token with a scoped credential pool (API Keys) issued via Dual-Signature login (`/admin/v1/login`). Scoped keys are prioritized; static fallback is restricted and flagged in production.

## 3. Resilience & Failure Modes

### 3.1 SRL-1 Recovery Triggers (Hole 3.1)
- **Status**: **COMPLETED v0.4.18**. Automatic recovery actions (Retry for transient errors, Split-Recovery for MPP failures, Reconciliation for indeterminate states) active via `AutonomousOrchestrator`.

## 4. Smart Contract Language & Enclave Evolution

### 4.1 Clarity 4 & Stacks Integration (CON-1200)
- **Concept**: Stacks 2.5/3.0 SIP alignment with passkey-based WebAuthn SECP256R1 authentication and contract bytecode hash verification.
- **Status**: Phase 1 Structural validation active in `src/executor/stacks.rs`.

### 4.2 Hardware Enclave Attestation Verification (Hole 2.1)
- **Concept**: Hardware-backed X.509 attestation certificate verification for confidential execution requests originating from TEE enclaves.
- **Status**: Structural DER validation active; `require_attestation` flag enforces hardware enclave attestation in production.

## 5. Sovereign Persistence & Storage Boundaries
- **Hole 1.2 (Redis Auth)**: **COMPLETED v0.4.18**. Enforced authenticated remote Redis connections in production release builds.
- **Tableland & Kwil**: Decentralized relational storage adapters for audit trails, state commitments, and sovereign OLTP persistence.

## 6. Emerging Research Areas (CON-1302, CON-1303, CON-1304, CON-1313)

### 6.1 Zero-Knowledge Contingent Payments (CON-1313 / G-50)
- **Concept**: Fair exchange of digital goods and secrets against Bitcoin/Lightning payments using SNARK SHA-256 pre-image circuit verification.
- **Status**: Scaffolding in `lib-conxian-core`.

### 6.2 FROST Threshold Signatures (CON-1302)
- **Concept**: Flexible Round-Optimized Schnorr Threshold Signatures for Taproot multi-party orchestration without revealing threshold policy structure on-chain.
- **Status**: Active research and protocol specification.

### 6.3 OP_CAT Recursive Covenants (CON-1303 / BIP-347)
- **Concept**: Taproot script execution with OP_CAT covenant tree verification for vault spending restrictions.
- **Status**: Active research and execution simulation.

### 6.4 Fedimint Community Liquidity (CON-1304)
- **Concept**: Federated blind signatures issuing untraceable e-cash for community privacy pools.
- **Status**: Phase 1 Federation Adapter active in `src/executor/fedimint.rs`.
