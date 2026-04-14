# SAB Migration & Governance Rollout — Conxian Nexus

This document defines the cutover from internal SAB-controlled bootstrap infrastructure to a decentralized, sovereign-operating model.

## 1. Wallet & Signer Transition [CON-419]
- **Current Bootstrap**: SPSZXAKV7DWTDZN2601WR31BM51BD3YTQWE97VRM
- **Target Governance**: Conxian DAO multi-sig.
- **Remediation**: All testnet defaults (ST...) removed. HD derivation in `lib-conxian-core` handles secure signing for settlement and rebalancing.

## 2. Orchestration & Deployment [CON-420]
- **Standard**: Mainnet-only production branches (`main`, `staged`).
- **Orchestration**: Docker-compose and K8s manifests updated to point to production Stacks RPC (`https://api.mainnet.hiro.so`).
- **Submodule Integrity**: Verified via CI guardrails.

## 3. Sovereign Infrastructure [CON-69, CON-473]
- **Relational State**: Migrating critical state commitment from hosted Postgres to **Tableland**.
- **Telemetry**: Autonomous telemetry reporting via **Nostr** to bypass centralized monitoring.
- **Health**: Decentralized status reporting implemented (NEXUS-04).

## 4. Mainnet Acceptance [CON-396]
- **Evidence Pack**: `docs/MAINNET_EVIDENCE.md` verified.
- **CI Guards**: Contamination guard active and passing.
