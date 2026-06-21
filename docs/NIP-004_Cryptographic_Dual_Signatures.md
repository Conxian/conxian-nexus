# NIP-004: Cryptographic Enforcement of Dual-Signatures for Admin API

## Status
Proposed (2026-06-21)

## Context
The current "Two-Person Control" (Dual-Signature) implementation in Conxian Nexus (`src/api/admin.rs`) relies on structural validation of JSON payloads. It checks for the presence of a second approver name and a list of signatures but does not verify the cryptographic validity of these signatures. This allows anyone with the `NEXUS_ADMIN_API_TOKEN` to forge approvals for critical actions like release decisions or governance changes.

## Proposal
Implement cryptographic signature verification for all `DualSignatureRequest` types.

1.  **Trusted Admin Keys**: Add an `admin_public_keys` configuration field to the `Config` struct, allowing the system to be initialized with a set of authorized public keys (secp256k1).
2.  **Signature Verification**: Update the `validate_dual_signature` trait method to:
    *   Require exactly two distinct signatures from the authorized keyset.
    *   Verify the signatures against a canonicalized hash of the request payload (or a specific "approval message").
3.  **Audit Trail**: Log the public keys of the approvers in the audit log.

## Implementation Details
- Use the `k256` crate (already in dependency tree) for Secp256k1 verification.
- Add `ADMIN_PUBLIC_KEYS_JSON` environment variable support.
- Canonicalize payloads using `serde_jcs` or a stable field-ordering approach before hashing.

## Consequences
- **Security**: Significantly hardens the Admin API against token theft and insider threats.
- **Complexity**: Increases the effort required to perform admin actions (requires signing tools).
- **Resilience**: Prevents accidental or unauthorized release of unverified protocol changes.

## References
- [CON-1202](https://linear.app/conxian-labs/issue/CON-1202)
- [HOLE_REPORT.md](./remediation/HOLE_REPORT.md)
