# NIP-006: Admin API Token Hardening and Rotation

## Status
Proposed (2026-06-21)

## Context
The `NEXUS_ADMIN_API_TOKEN` is a static, long-lived bearer token. Its exposure would grant full administrative access to the Glass Node.

## Proposal
1.  **Short-Lived Tokens**: Transition the Admin API to use ephemeral JWTs issued via an authenticated `/admin/v1/login` flow.
2.  **MFA/Hardware Support**: Allow the `login` flow to require a signature from a pre-authorized hardware-backed public key (aligning with NIP-004).
3.  **Role-Based Access Control (RBAC)**: Split the `admin.write` scope into more granular permissions (e.g., `release.approve`, `gov.submit`, `sys.reboot`).

## Implementation Details
- Use `jsonwebtoken` crate for token issuance and validation.
- Store authorized admin public keys in the centralized `Config`.

## References
- [CON-1202](https://linear.app/conxian-labs/issue/CON-1202)
- [SECURITY_HARDENING.md](./remediation/SECURITY_HARDENING.md)
