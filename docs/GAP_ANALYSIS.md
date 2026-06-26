# Conxian Nexus: Gap Analysis & Research Map (v0.4.17)

This document maps identified security holes and protocol gaps to their research foundations and provides a prioritization score.

## 1. Scorecard

| Gap ID | Description | Impact (1-10) | Effort (1-10) | Priority | Status |
|---|---|---|---|---|---|
| **NIP-007** | Safety Mode Enforcement in Submission Path | 9 | 1 | **P0** | **Completed** |
| **NIP-004** | Cryptographic Dual-Signature Verification | 10 | 5 | **P0** | **Completed** |
| **Hole 4.1** | MEV Audit Detail Expansion | 6 | 1 | **P1** | **Completed** |
| **NIP-005** | Real Multi-Chain Verification (Tier 1) | 9 | 9 | **P1** | **Initializing (BitVM2)** |
| **G-09** | BIP-322 Universal Message Signing (CON-1266) | 7 | 4 | **P1** | **Completed** |
| **G-50** | ZKCP Implementation (CON-1313) | 8 | 7 | **P1** | Backlog |
| **NIP-006** | Admin Token Hardening (JWT/RBAC) | 8 | 6 | **P1** | Proposed |
| **G-43** | Babylon Staking Adapter (CON-1312) | 7 | 5 | **P2** | **Completed** |
| **Hole 1.2** | Authenticated Redis & Enclave Isolation | 7 | 4 | **P2** | Backlog |

## 2. Mapping & Research Context

### 2.1 Safety Mode Enforcement (NIP-007)
- **Gap**: `NexusExecutor::submit` ignores the `is_safety_mode_active` flag.
- **Status**: **Resolved v0.4.17**.
- **Code**: `src/executor/mod.rs`

### 2.2 Cryptographic Dual-Signatures (NIP-004)
- **Gap**: Initial implementation was structural only.
- **Status**: **Resolved v0.4.17**. Cryptographic Secp256k1 verification is fully integrated using `k256`.
- **Code**: `src/api/admin.rs`

### 2.3 Multi-Chain Verification (NIP-005)
- **Gap**: Adapters for BitVM2, EVM, and Cosmos are stubs.
- **Best Candidate**: BitVM2 Groth16 verification via `ark-groth16`.
- **Status**: Initial cryptographic verification implemented in `src/executor/bitvm.rs`.
- **Code**: `src/executor/{bitvm, evm, cosmos}.rs`

### 2.4 ZKCP Implementation (G-50)
- **Gap**: Trustless information-for-value exchange on Bitcoin.
- **Research**: Requires Discreet Log Contracts (DLC) or specific script patterns.
