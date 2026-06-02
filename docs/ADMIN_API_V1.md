# Admin API v1

This document introduces the first runtime-side admin API surface for BOS control-plane workflows in `conxian-nexus`.

## Routes

### Protected admin routes
- `GET /admin/v1/status`
- `POST /admin/v1/releases/request-approval`
- `POST /admin/v1/releases/decision`
- `POST /admin/v1/governance/decision`

### Agent-to-product discovery and registration
- `GET /auth.md`
- `GET /.well-known/oauth-protected-resource`
- `GET /.well-known/oauth-authorization-server`
- `POST /agent/auth`
- `POST /agent/auth/claim`
- `POST /agent/auth/claim/complete`
- `GET /agent/auth/claim/view?token=...`

## Current behavior

This is a bootstrap integration layer with working end-to-end router coverage.

It currently supports:
- auth.md discovery
- OAuth protected resource metadata
- OAuth authorization server metadata with `agent_auth`
- anonymous registration with pre-claim `api.read` scope
- verified-email registration with post-claim credential issuance
- claim ceremony with OTP view endpoint
- protected bearer-credential access to admin routes
- `WWW-Authenticate` metadata hints on unauthorized protected requests

## Auth behavior

- If `NEXUS_ADMIN_API_TOKEN` is configured, that bearer token is accepted for admin access.
- Agent-issued bearer credentials are also accepted when scopes satisfy the route.
- Anonymous registration yields a pre-claim credential with `api.read` only.
- Completing claim upgrades or issues credentials with `api.read` and `api.write`.

## Purpose

This gives the BOS control-plane work in `conxian-business` a concrete runtime-side contract landing zone while keeping privileged execution out of the UI.

## Follow-up

- replace the bootstrap OTP/view flow with durable user delivery
- persist audit events durably
- enforce richer authorization server-side
- connect decisions to trusted downstream orchestration paths
- add expiry, revocation, and persistent registration storage
