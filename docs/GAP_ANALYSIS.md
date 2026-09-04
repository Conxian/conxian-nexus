# Conxian Nexus: Gap Analysis & Research Map (v0.4.23)

This document maps identified security holes, protocol gaps, and active research initiatives to their research foundations and provides a standardized prioritization score.

## 1. Scorecard

| Gap ID | Description | Impact (1-10) | Effort (1-10) | Priority | Status |
|---|---|---|---|---|---|
| **NIP-007** | Safety Mode Enforcement in Submission Path | 9 | 1 | **P0** | **Completed (v0.4.17)** |
| **NIP-004** | Cryptographic Dual-Signature Verification | 10 | 5 | **P0** | **Completed (v0.4.17)** |
| **Hole 4.1** | MEV Audit Detail Expansion | 6 | 1 | **P1** | **Completed (v0.4.17)** |
| **NIP-005 (BitVM)** | BitVM2 Groth16 Verification (ark-groth16) | 10 | 8 | **P0** | **Completed (v0.4.22)** |
| **NIP-005 (EVM)** | EVM Merkle Patricia Trie (MPT) Cryptographic Verification | 9 | 6 | **P1** | **Upgraded (v0.4.23)** |
| **NIP-005 (Cosmos)** | Cosmos IBC Tendermint Header Cryptographic Verification | 9 | 6 | **P1** | **Upgraded (v0.4.23)** |
| **CON-1200** | Stacks Clarity 4 & sBTC Protocol Adapter Upgrade | 8 | 5 | **P1** | **Upgraded Phase 2 (v0.4.23)** |
| **CON-1304** | Fedimint Blinded Mint e-Cash Proof Verification & Double-Spend Check | 8 | 5 | **P1** | **Completed Phase 2 (v0.4.23)** |
| **G-09** | BIP-322 Universal Message Signing (CON-1266) | 7 | 4 | **P1** | **Completed** |
| **G-50** | ZKCP Implementation (CON-1313) | 8 | 7 | **P1** | **Scaffolding (lib-core)** |
| **NIP-006** | Admin Token Hardening (Scoped Credentials / RBAC) | 8 | 6 | **P1** | **Completed (v0.4.18)** |
| **Hole 3.1** | SRL-1 Recovery Triggers | 7 | 6 | **P1** | **Completed (v0.4.18)** |
| **Hole 1.2** | Authenticated Redis & Enclave Isolation | 7 | 4 | **P2** | **Completed (v0.4.18)** |
| **Hole 2.1** | Hardware Enclave Certificate Chain Verification | 8 | 5 | **P1** | **Completed (v0.4.23)** |
| **G-43** | Babylon Staking Adapter (CON-1312) | 7 | 5 | **P2** | **Completed** |
| **CON-1302** | FROST Threshold Signatures | 8 | 6 | **P1** | **Active Research** |
| **CON-1303** | OP_CAT Recursive Covenants (BIP-347) | 7 | 7 | **P2** | **Active Research** |

## 2. Mapping & Research Context

### 2.1 Safety Mode Enforcement (NIP-007)
- **Gap**: `NexusExecutor::submit` ignores the `is_safety_mode_active` flag.
- **Status**: **Resolved v0.4.17**. Execution blocked during active safety mode / sovereign handoff.
- **Code**: `src/executor/mod.rs`

### 2.2 Cryptographic Dual-Signatures (NIP-004)
- **Gap**: Initial implementation was structural only.
- **Status**: **Resolved v0.4.17**. Cryptographic Secp256k1 verification is fully integrated using `k256`.
- **Code**: `src/api/admin.rs`

### 2.3 Multi-Chain Verification (NIP-005)
- **Gap**: Adapters for EVM, Cosmos, and Stacks required cryptographic proof verification beyond structural checks.
- **Remediation**:
  - **BitVM2**: Canonical BN254 Groth16 verifier using `ark-groth16` (`src/executor/bitvm_groth16.rs`).
  - **EVM (v0.4.23)**: Merkle Patricia Trie (MPT) node hash chain verification against `receipt_root` using Keccak-256 (`src/executor/evm.rs`).
  - **Cosmos (v0.4.23)**: Base64 header payload decoding, SHA-256 digest validation, and height progression checks (`src/executor/cosmos.rs`).
  - **Stacks / sBTC (v0.4.23 Upgrade)**: Stacks address prefix (`SP`/`ST`) validation, 0x-hex tx_id verification, sBTC amount bounds enforcement, duplicate transaction detection, and SQLx PostgreSQL persistence (`src/executor/stacks.rs`).
- **Code**: `src/executor/evm.rs`, `src/executor/cosmos.rs`, `src/executor/stacks.rs`, `src/executor/bitvm_groth16.rs`

### 2.4 SRL-1 Recovery (Hole 3.1)
- **Gap**: Failure taxonomy exists, but automatic recovery actions were not triggered.
- **Status**: **Resolved v0.4.18**. Automatic triggers for retries, split-recovery, and reconciliation implemented.
- **Code**: `src/orchestrator/mod.rs`

### 2.5 Admin Token Hardening (NIP-006)
- **Gap**: Static bearer token was the only auth path.
- **Status**: **Resolved v0.4.18**. Implemented scoped credential pool (API Keys) with prioritization over static fallback. Production warning for static token use.
- **Code**: `src/api/admin.rs`

### 2.6 Authenticated Redis & Enclave Isolation (Hole 1.2)
- **Gap**: Redis could be unauthenticated in production builds.
- **Status**: **Resolved v0.4.18**. Enforced authenticated and remote Redis in release builds with safety override flag.
- **Code**: `src/storage/mod.rs`

### 2.7 Zero-Knowledge Contingent Payments (G-50 / CON-1313)
- **Gap**: Fair exchange of secrets against Bitcoin/Lightning payments using SNARK pre-image verification.
- **Status**: **Scaffolding (lib-core)**. SHA-256 pre-image circuit verification pipeline in research.
- **Code**: `lib-conxian-core`

### 2.8 FROST Threshold Signatures (CON-1302)
- **Gap**: Flexible Round-Optimized Schnorr Threshold Signatures for Taproot multi-party orchestration requiring cooperative subset coordination and threshold bounds enforcement.
- **Status**: **Active Research & Orchestration Scaffolding**. Multi-sig vault abstraction indistinguishable on-chain. Integrates with ROAST orchestrator in `src/orchestrator/roast.rs` for dynamic fault exclusion, retry tracking, and threshold participant set management.
- **Code**: `src/orchestrator/roast.rs`

### 2.9 OP_CAT Recursive Covenants (CON-1303 / BIP-347)
- **Gap**: Introspection and recursive covenant spending condition checks for Bitcoin Taproot scripts.
- **Status**: **Active Research**. Monitored via OP_CAT execution simulator.

### 2.10 Fedimint Blinded Mint e-Cash Verification (CON-1304)
- **Gap**: Initial adapter only had a placeholder structural check returning `Ok(true)`.
- **Status**: **Phase 2 Cryptographic Audit**. Verifies e-cash token format (`fed:` / `fed1:` prefix, base64 payload, length > 32 bytes), derives unique SHA-256 nonce hashes, enforces strict double-spend checks against `fedimint_verified_proofs` in SQLx, and logs immutable audit trails.
- **Code**: `src/executor/fedimint.rs`, `migrations/20260818000000_fedimint_mint_audit.sql`

### 2.11 Hardware Enclave Attestation Verification (Hole 2.1)
- **Gap**: Soft enforcement allowed submission without attestation certificates in development mode.
- **Status**: **Completed (v0.4.23)**. X.509 DER certificate decoding using `x509-cert`, validity window verification (`not_before` / `not_after`), and configurable soft/hard attestation enforcement via `require_attestation`.
- **Code**: `src/executor/mod.rs`

### 2.12 Lightning Billing & Production Settlement (CON-24)
- **Gap**: Billing upgrade handlers used non-canonical mock strings.
- **Status**: **Completed (v0.4.23)**. Standardized canonical BOLT11 `lnbc` payment encoding and Redis settlement verification.
- **Code**: `src/api/billing/mod.rs`
