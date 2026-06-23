# Conxian Nexus: Gap Analysis & Research Map (v0.4.16)

This document maps identified security holes and protocol gaps to their research foundations and provides a prioritization score.

## 1. Scorecard

| Gap ID | Description | Impact (1-10) | Effort (1-10) | Priority | Status |
|---|---|---|---|---|---|
| **NIP-007** | Safety Mode Enforcement in Submission Path | 9 | 2 | **P0** | Initializing |
| **NIP-004** | Cryptographic Dual-Signature Verification | 10 | 5 | **P0** | Proposed |
| **Hole 4.1** | MEV Audit Detail Expansion | 6 | 3 | **P1** | Initializing |
| **NIP-006** | Admin Token Hardening (JWT/RBAC) | 8 | 6 | **P1** | Proposed |
| **NIP-005** | Real Multi-Chain Verification (Tier 1) | 9 | 9 | **P2** | Proposed |
| **Hole 1.2** | Authenticated Redis & Enclave Isolation | 7 | 4 | **P2** | Backlog |

## 2. Mapping & Research Context

### 2.1 Safety Mode Enforcement (NIP-007)
- **Gap**: `NexusExecutor::submit` ignores the `is_safety_mode_active` flag.
- **Research**: Foundational "Fail-Closed" security pattern.
- **Code**: `src/executor/mod.rs`

### 2.2 Cryptographic Dual-Signatures (NIP-004)
- **Gap**: `src/api/admin.rs` performs structural validation only.
- **Research**: NIP-004 identifies the need for Secp256k1 verification of the second approver.
- **Code**: `src/api/admin.rs`, `src/config.rs`

### 2.3 MEV Audit Detail (Hole 4.1)
- **Gap**: `me_audit_log` is too sparse for forensics.
- **Research**: MEV Transparency research (v0.4.0 roadmap) requires full payload and sequencing metadata.
- **Code**: `src/executor/mod.rs`, `migrations/202401010005_mev_audit.sql`

### 2.4 Multi-Chain Verification (NIP-005)
- **Gap**: Adapters for BitVM2, EVM, and Cosmos are stubs.
- **Research**: ADR-006 defines Tier 1 priorities. NIP-005 outlines the transition to real crypto.
- **Code**: `src/executor/{bitvm, evm, cosmos}.rs`
