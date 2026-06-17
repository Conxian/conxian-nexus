# Conxian Nexus Observability & Incident Runbook

## Overview
This runbook provides guidance for monitoring Nexus Glass Node health and responding to common production incidents.

## Key Observability Signals

### 1. Sync Drift (Nexus-Safety)
- **Metric**: `nexus_sync_height_drift`
- **Alarm**: Trigger if drift > 2 blocks for > 5 minutes.
- **Action**: Verify Stacks RPC endpoint reachability. Check `nexus-sync` logs for reorg-rollback cycles.

### 2. Transaction Throughput
- **Metric**: `nexus_transactions_total` (Counter)
- **Alarm**: Trigger if rate drops to 0 for > 15 minutes during active windows.
- **Action**: Inspect `nexus-executor` logs for sequencing blocks or validation failures.

### 3. Oracle Confidence
- **Metric**: `nexus_oracle_confidence_score`
- **Alarm**: Trigger if any Tier 1 pair confidence < 0.5.
- **Action**: Check status of external feed providers (ExchangeRate.host, etc.).

## Incident Response Procedures

### Scenario A: Safety Mode Triggered
1. **Diagnosis**: Check if Nexus is in "Safety Mode" via `/v1/status`.
2. **Action**: Stop automated settlement triggers.
3. **Recovery**: Once sync drift is < 2, Nexus will automatically exit Safety Mode. Acknowledge the event via the Admin API.

### Scenario B: Database Connection Loss
1. **Diagnosis**: `sqlx` errors in logs.
2. **Action**: Nexus uses lazy connection pooling but will fail-closed for state commitments.
3. **Recovery**: Restart Docker containers to refresh pool. Verify PostgreSQL persistence via `pg_isready`.

### Scenario C: Single-Key Admin Breach
1. **Diagnosis**: Unauthorized changes in `me_audit_log`.
2. **Action**: Immediately rotate `NEXUS_ADMIN_API_TOKEN`.
3. **Recovery**: Transition to the Multi-Sig/DAO governance model as per CON-1202.
