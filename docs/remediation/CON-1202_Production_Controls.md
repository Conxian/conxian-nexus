# Remediation: Single-Key Protocol Admin Risk (CON-1202)

## Issue
The current production posture relies on a single deployer or admin key for critical contract control. This represents a central point of failure and a security risk.

## Findings
- **Current State**: Admin routes in `src/api/admin.rs` use a single bearer token (`NEXUS_ADMIN_API_TOKEN`).
- **Target State**: Transition to a Multi-Signature (Multi-Sig) or Decentralized Autonomous Organization (DAO) controlled administration model.
- **Interim Control**: Hardened access logging and threshold-based verification for critical protocol changes.

## Remediation Plan
1. **Credential Federation**: Transition the Admin API to support federated identity assertions (e.g., OIDC or Nostr-signed claims) rather than a static token.
2. **Action Thresholds**: Implement "Two-Person Control" for critical endpoints (e.g., `release/approval` or `config/update`). This requires signatures from two distinct authorized agents.
3. **Audit Trail**: Redirect all admin actions to a persistent, immutable log (e.g., Tableland) to ensure an auditable history of protocol changes.
4. **DAO Handoff**: Prepare the `ContractBridge` to hand off the `admin` role to a Clarity-based Multi-Sig contract once the Clarity 4 verification gap (CON-1200) is resolved.

## Verification Checklist
- [ ] Multi-agent signing logic implemented in `src/api/admin.rs`.
- [ ] Admin actions persisted to decentralized storage.
- [ ] Documentation updated for "Ceremony" procedures.
