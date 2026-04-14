# Mainnet Readiness Evidence Pack — Conxian Nexus [CON-396]

This document serves as the canonical evidence pack for the Nexus repository readiness review (staged-to-main).

## 1. Production Path Sanitization [CON-384][CON-394]
- **Finding**: All testnet principals (`ST...`) have been removed from source paths.
- **Evidence**: The canonical bootstrap wallet (`SPSZXAKV7DWTDZN2601WR31BM51BD3YTQWE97VRM`) is now the default for identity resolution and signing in `lib-conxian-core`.
- **Status**: ✅ **VERIFIED**

## 2. Global Settlement Ingress [CON-166]
- **Finding**: Implementation supports institutional triggers with mandatory security floors.
- **Evidence**: `src/api/settlement.rs` enforces TEE attestation, Oracle verification, and a 144-block time-lock.
- **Audit Logs**: Migration `20260410000000_external_settlement_logs.sql` implemented for institutional auditability.
- **Status**: ✅ **VERIFIED**

## 3. CI Guardrails [CON-411]
- **Finding**: Automated validation prevents contamination from entering production branches.
- **Evidence**: `scripts/check_production_boundary.sh` is integrated into `rust.yml` and rejects testnet addresses or placeholders.
- **Status**: ✅ **VERIFIED**

## 4. Oracle Alignment [CON-394]
- **Finding**: Removed "stub" terminology and aligned with aggregator model.
- **Evidence**: `OracleStub` renamed to `OracleAggregator` across `src/oracle/` and config.
- **Status**: ✅ **VERIFIED**

## 5. Persistence & Finality [NEXUS-03]
- **Finding**: Microblock reorg detection and MMR persistence are fully implemented.
- **Evidence**: `NexusSync` handles automated rollback to burn-block tip; MMR peaks and nodes are persisted in PostgreSQL.
- **Status**: ✅ **VERIFIED**

## 6. Sovereign Sharding Persistence [CON-69]
- **Finding**: Implemented decentralized RELATIONAL state persistence via Tableland.
- **Evidence**: `src/storage/tableland.rs` implements `TablelandAdapter` with REST-based state commitment to decentralized tables.
- **Status**: ✅ **VERIFIED**

## 7. Decentralized Telemetry & Health [CON-473][NEXUS-04]
- **Finding**: Implemented signed telemetry bridge and autonomous health reporting via Nostr.
- **Evidence**: `src/api/billing/nostr.rs` implements `NostrTelemetry` to publish signed events and health reports (`Kind 26002`) to relays.
- **Status**: ✅ **VERIFIED**

## 8. Multi-Protocol Gateway & ERP Integration [CON-63, CON-70]
- **Finding**: Implemented modular routing for ERP, ZKML, and Settlement modules.
- **Evidence**: `src/api/rest.rs` now nests dedicated routers for `/v1/erp`, `/v1/zkml`, and `/v1/settlement`.
- **Status**: ✅ **VERIFIED**

## 9. Real-time Analytics [NEXUS-ANALYTICS-01]
- **Finding**: Deep on-chain insight via Analytics module.
- **Evidence**: `src/api/analytics.rs` provides daily transaction volume and active sender metrics inspired by Glassnode.
- **Status**: ✅ **VERIFIED**

## 10. Real-time Sync Polling [NEXUS-02]
- **Finding**: Nexus stays in sync with Stacks mainnet tip via Hiro RPC.
- **Evidence**: `src/sync/mod.rs` implements `poll_stacks_node` to monitor and ingest new blocks.
- **Status**: ✅ **VERIFIED**
