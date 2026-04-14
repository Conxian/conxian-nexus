# SAB Infrastructure Migration: Nexus Dependency Inventory [CON-329][CON-337]

This document maps the Conxian Nexus dependencies from Web2 infrastructure (Neon/Supabase) to their sovereign target state (Kwil/Tableland).

## 1. Database Table Inventory (Neon/Supabase)

| Table Name | Business Function | Data Domain | Current Role | Target State (Sovereign) | Migration Risk |
| ---------- | ----------------- | ----------- | ------------ | ------------------------ | -------------- |
| `mmr_nodes` | Cryptographic Proofs | State Integrity | Transactional | Kwil | Medium |
| `mmr_peaks` | Cryptographic Proofs | State Integrity | Transactional | Kwil | Medium |
| `nexus_state_roots` | historical Audit | State Integrity | Transactional | Kwil / Tableland | Low |
| `cxn_external_settlement_logs` | Institutional Audit | Settlement | Analytical | Kwil | Low |
| `settlement_proposals` | Time-lock Enforcement | Settlement | Transactional | Kwil | High |
| `mev_audit_log` | MEV Transparency | Security | Analytical | Tableland | Low |
| `oracle_fx_history` | Price Feed Tracking | Oracle | Time-series | Kwil | Low |
| `stacks_blocks` | L1 State Cache | Protocol | Cache | Kwil | Low |
| `stacks_transactions` | L1 State Cache | Protocol | Cache | Kwil | Low |

## 2. Infrastructure Services

| Service | Current Provider | Target State | Responsibility |
| ------- | ---------------- | ------------ | -------------- |
| Relational SQL | Neon (AWS) | Kwil | Transactional state and audit logs. |
| State Commitment | Manual/Postgres | Tableland | Publicly verifiable state roots. |
| Caching / PubSub | Redis (Rented) | Local Sovereign Redis | Fast state access and internal event bus. |

## 3. Migration Roadmap (Pilot Wave)

1. **Phase 1 (Complete)**: Implement `KwilAdapter` and verify basic block/root persistence logic.
2. **Phase 2 (Planned)**: Migrate `mmr_nodes` and `mmr_peaks` to Kwil to test performance of large-volume cryptographic writes.
3. **Phase 3 (Planned)**: Full cutover of `settlement_proposals` to Kwil to ensure transactional integrity during handoff.

## 4. Decision Log

- **Why Kwil?**: Chosen as the primary relational layer due to its ability to handle high-frequency transactional data with sovereign SQL guarantees.
- **Why Tableland?**: Reserved for public audit logs (`mev_audit_log`) and state root anchoring where cross-jurisdictional verifiability is more important than raw write performance.
