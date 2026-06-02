# Admin API v1

This document introduces the first runtime-side admin API surface for BOS control-plane workflows in `conxian-nexus`.

## Routes

- `POST /admin/v1/releases/request-approval`
- `POST /admin/v1/releases/decision`
- `POST /admin/v1/governance/decision`

## Current behavior

This is a bootstrap integration layer.

The handlers currently:
- accept typed JSON requests
- return accepted responses with generated request/decision IDs
- return generated audit event IDs
- optionally require a bearer token when `NEXUS_ADMIN_API_TOKEN` is configured
- do **not** claim settlement, promotion, signing, or downstream execution success

## Purpose

This gives the BOS control-plane work in `conxian-business` a concrete runtime-side contract landing zone while keeping privileged execution out of the UI.

## Follow-up

- replace the token placeholder with real authenticated actor/session validation
- add durable audit event persistence
- enforce authorization server-side
- connect decisions to trusted downstream orchestration paths
- expand tests for request validation and failure modes
