# Conxian Nexus: Security Hardening Summary (v0.4.18)

This document tracks the systematic hardening of the Conxian Nexus "Glass Node" to ensure its integrity as a core protocol component.

## 1. Administrative Authorization (NIP-004, NIP-006)

### Scoped API Keys & Dual-Sig Login
Nexus has transitioned from a single static `NEXUS_ADMIN_API_TOKEN` to a dynamic, scoped credential pool.
- **Login Mechanism**: Administrators must use the `/admin/v1/login` endpoint, which requires a cryptographic Dual-Signature (Secp256k1) matching two distinct keys in the configured `ADMIN_PUBLIC_KEYS`.
- **Scoped Credentials**: Successful login issues an `nx_key_...` token with specific scopes (`admin.write`, `api.read`, `api.write`).
- **Audit Trail**: Every login and subsequent write action is instrumented with `tracing` to capture caller intent and session context.
- **Fail-Closed Fallback**: The legacy static token is now a fallback restricted primarily to development. In production-like builds, its use triggers a critical warning.

## 2. Infrastructure Boundary (Hole 1.2)

### Authenticated Persistence
The storage layer (`src/storage/mod.rs`) now enforces strict connection rules for Redis and PostgreSQL in release builds.
- **Mandatory Auth**: Connections to `127.0.0.1` or `localhost` without a password (or unauthenticated schemes) are rejected in non-debug environments for both Redis and PostgreSQL.
- **Override Path**: Emergency access is available via the `NEXUS_ALLOW_UNSAFE_REDIS=1` and `NEXUS_ALLOW_UNSAFE_DB=1` environment flags, ensuring intentionality during infrastructure failures.

## 3. Resilience & Recovery (Hole 3.1)

### SRL-1 Automatic Triggers
The `AutonomousOrchestrator` now actively polls and recovers stale or failed Lightning payments.
- **Automated Retry**: Transient failures trigger up to 3 retries with exponential backoff.
- **Split Recovery**: Partial MPP failures trigger residual routing sequences.
- **Reconciliation**: Indeterminate states are resolved via automated node lookups after a 60-second window.

## 4. Multi-Chain Verification (NIP-005)

### Real BitVM2 SNARK Verification
Transitioned BitVM2 from structural validation to full cryptographic verification using `ark-groth16`.
- **Artifact Auditing**: `vk_hash` and `public_inputs_hash` are now captured in the permanent audit log.
- **Zero-Knowledge Evidence**: Transition proofs are cryptographically verified against the provided SNARK circuit.

## 5. MEV & Sequencing Transparency (Hole 4.1)

### Expanded Audit Detail
The sequencer's internal audit log (`me_audit_log`) has been expanded to ensure full transparency of handled transactions.
- **Full Payload Capture**: The entire transaction body is persisted for post-hoc analysis.
- **Priority Metadata**: Sequencing priority is recorded to detect and prevent unauthorized front-running.
