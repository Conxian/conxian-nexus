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
