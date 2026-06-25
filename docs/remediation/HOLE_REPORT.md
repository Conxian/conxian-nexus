# Conxian Nexus: Vulnerability and Resilience Audit Report (CON-1202)

## 1. Administrative Control Bypasses (Critical)

### Hole 1.1: Non-Cryptographic Dual Signature
The "Two-Person Control" implementation in `src/api/admin.rs` was previously performing structural validation only.
- **Status**: **Resolved v0.4.17 (NIP-004)**. `validate_dual_signature` now enforces cryptographic Secp256k1 verification of exactly two distinct authorized signatures against the `admin_public_keys` set.
- **Remediation**: Transitioned to `k256` based ECDSA verification.

### Hole 1.2: Static Admin Bearer Token
The system relies on a single environment-provided `NEXUS_ADMIN_API_TOKEN`.
- **Vulnerability**: If this token is leaked, the entire administrative surface (releases, governance) is compromised.
- **Remediation**: Move to ephemeral, session-based tokens (JWT) or hardware-backed signatures for all write operations. Proposed as NIP-006.

## 2. Multi-Chain Verification Gaps (High)

### Hole 2.1: Simulated Verification Floors
All Tier 1 Protocol Adapters (`bitvm.rs`, `evm.rs`, `cosmos.rs`, `stacks.rs`) are currently stubs.
- **Vulnerability**: The system reports "Verified" for any validly formatted input. This creates a "false sense of finality" for downstream consumers (Gateway/UI).
- **Remediation**: Integrate real verification libraries (e.g., `arkworks` for BitVM2, Tendermint light client logic for Cosmos). Proposed as NIP-005.

## 3. Resilience & Failure Modes (Medium)

### Hole 3.1: Incomplete SRL-1 Action Logic
While `src/executor/lightning.rs` has a failure taxonomy, the `NexusExecutor` does not yet trigger automatic recovery or state rollbacks based on these failures.
- **Vulnerability**: Partial failures in MPP payments may leave the system in an inconsistent state if the observer doesn't act on the `MppPartial` classification.
- **Remediation**: Implement the state machine transitions defined in `ADR-006` to handle `Recovering` and `MppSplitting` states.

## 4. Storage & Persistence (Medium)

### Hole 4.1: Missing MEV Audit Detail
The `me_audit_log` previously captured only the payload hash and sender.
- **Status**: **Resolved v0.4.17**. Audit log now includes the full transaction body and sequencing priority metadata for better forensics.
- **Remediation**: Expanded schema and `NexusExecutor::submit` logic.
