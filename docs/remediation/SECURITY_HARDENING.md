# Conxian Nexus Security Hardening (CON-1202)

## 1. Zero-Secret Logging
- **Rule**: No cryptographic materials (PEM, KEY, PFX, etc.) allowed in debug logs.
- **Implementation**: The `Config` struct in `src/config.rs` uses a custom `Debug` implementation to redact sensitive fields.

## 2. Admin API Token Rotation
- **Current**: Single bearer token (`NEXUS_ADMIN_API_TOKEN`).
- **Required**: Transition to ephemeral, short-lived tokens issued via the `identity/register` flow.
- **Migration Path**:
  - Phase 1: Enable multi-agent logging (implemented).
  - Phase 2: Require dual-signature for `release/approval`.
  - Phase 3: Move admin authority to a Clarity Multi-Sig.

## 3. Production Boundary
- **Rule**: No `ST...` (Testnet) addresses in production paths.
- **Check**: Enforced via `scripts/check_production_boundary.sh`.

## 4. Dependency Security
- **Rule**: Critical vulnerabilities (e.g., CVE-2023-44487) remediated immediately.
- **Status**: `openssl` and `h2` patched to latest verified versions in June 2026.
