# Conxian Nexus: Vulnerability and Resilience Audit Report (CON-1202)

## 1. Administrative Control Bypasses (Critical)

### Hole 1.1: Non-Cryptographic Dual Signature
The "Two-Person Control" implementation in `src/api/admin.rs` performs structural validation only.
- **Vulnerability**: The `validate_dual_signature` method checks if a `second_approver` string is present and if the `signatures` array has at least 2 items. It does **not** verify the content of the signatures or the identity of the approvers via public keys.
- **Attack Vector**: An attacker with the `NEXUS_ADMIN_API_TOKEN` can forge the JSON payload with arbitrary strings to bypass the dual-signature gate.
- **Remediation**: Transition to ECDSA/Schnorr signature verification using a pre-configured set of admin public keys.

### Hole 1.2: Static Admin Bearer Token
The system relies on a single environment-provided `NEXUS_ADMIN_API_TOKEN`.
- **Vulnerability**: If this token is leaked, the entire administrative surface (releases, governance) is compromised.
- **Remediation**: Move to ephemeral, session-based tokens (JWT) or hardware-backed signatures for all write operations.

## 2. Multi-Chain Verification Gaps (High)

### Hole 2.1: Simulated Verification Floors
All Tier 1 Protocol Adapters (`bitvm.rs`, `evm.rs`, `cosmos.rs`, `stacks.rs`) are currently stubs.
- **Vulnerability**: The system reports "Verified" for any validly formatted input. This creates a "false sense of finality" for downstream consumers (Gateway/UI).
- **Remediation**: Integrate real verification libraries (e.g., `arkworks` for BitVM2, Tendermint light client logic for Cosmos).

## 3. Resilience & Failure Modes (Medium)

### Hole 3.1: Incomplete SRL-1 Action Logic
While `src/executor/lightning.rs` has a failure taxonomy, the `NexusExecutor` does not yet trigger automatic recovery or state rollbacks based on these failures.
- **Vulnerability**: Partial failures in MPP payments may leave the system in an inconsistent state if the observer doesn't act on the `MppPartial` classification.
- **Remediation**: Implement the state machine transitions defined in `ADR-006` to handle `Recovering` and `MppSplitting` states.

## 4. Storage & Persistence (Medium)

### Hole 4.1: Missing MEV Audit Detail
The `me_audit_log` captures only the payload hash and sender.
- **Vulnerability**: Insufficient data to reconstruct complex reordering attacks or MEV extraction on the Nexus sequencer.
- **Remediation**: Expand audit logs to include the full transaction body (encrypted if necessary) and sequencing priority metadata.
