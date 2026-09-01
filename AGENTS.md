# Conxian Nexus: Agent Instructions

You are working on **Conxian Nexus**, the protocol-first "Glass Node" proof layer for Tier 1 multi-chain observation, synchronization, and verification in the Conxian ecosystem.

## Core Identity
- **Role**: Authoritative off-chain state aligned with multi-chain activity via cryptographic state root commitments (MMR).
- **Governance**: Maintained by Conxian-Labs as public infrastructure. BUSL-1.1 licensed (Change Date: 2030-01-01 → GPL-3.0-or-later).
- **Boundary Rule**: Nexus observes and proves — it does NOT execute. Execution is the Gateway's domain.

## Architecture

```
Bitcoin/EVM/Cosmos —→ sync module —→ MMR state roots —→ REST/gRPC API
                          │
                    executor adapters (BitVM2, RGB, Stacks, Lightning, Fedimint, EVM, Cosmos)
                          │
                    SRL-1 safety layer (drift monitoring, resilience)
```

## Module Map

| Module | Purpose | Status |
|--------|---------|--------|
| `nexus-sync` | Multi-chain ingestion, reorg handling (BTC/EVM/Cosmos) | Active |
| `nexus-state` | MMR state root commitments, persistence, Redis + PostgreSQL | Active |
| `nexus-executor` | Protocol adapters: BitVM2, RGB, Stacks, Lightning, Fedimint, EVM, Cosmos | Active |
| `nexus-safety` | Drift monitoring, SRL-1 Lightning resilience layer | Active |
| `api` | REST + gRPC surfaces for proofs, event feeds, identity, settlement, ZKML, DLC, ERP | Active |
| `storage` | Tableland + Kwil adapters for MMR node persistence | Active |

## Protocol Coverage — SDK → Nexus Alignment

The Conxius Enclave SDK (`conxius-enclave-sdk` v2.0.17) defines the canonical 42-chain AssetRegistry and 43 protocol modules (25 blockchain + 18 infrastructure). Nexus must maintain observation/proof coverage for all chains where Conxian holds state.

### Chain Coverage (42 SDK chains → Nexus observation status)

| Chain | SDK Registry | Nexus Status | Notes |
|-------|-------------|-------------|-------|
| Bitcoin | ✅ BTC | ✅ Full | Primary L1 anchor |
| Stacks | ✅ STX | ✅ Executor | StacksAdapter active |
| Lightning | ✅ BTC | ✅ SRL-1 | LightningResilienceAdapter active |
| Liquid | ✅ L-BTC | ⚠️ Via Gateway | Gateway-owned adapter |
| Rootstock | ✅ RBTC | ⚠️ Via Gateway | Gateway-owned adapter |
| BOB | ✅ BTC | ⚠️ Planned | Bitcoin L2 observation |
| Babylon | ✅ BTC | ⚠️ Via Gateway | Staking observation |
| Botanix | ✅ BTC | ⚠️ Planned | Spiderchain L2 |
| Citrea | ✅ BTC | ⚠️ Via Gateway | ZK-rollup observation |
| Mezo | ✅ BTC | ⚠️ Planned | Bitcoin L2 |
| Ethereum | ✅ ETH | ✅ EVM | EVMAdapter active |
| Solana | ✅ SOL | ⚠️ Planned | SolanaAdapter P2 |
| Polygon | ✅ POL | ✅ EVM | Via EVMAdapter |
| BSC | ✅ BNB | ✅ EVM | Via EVMAdapter |
| Avalanche | ✅ AVAX | ✅ EVM | Via EVMAdapter |
| Arbitrum | ✅ ETH | ✅ EVM | Via EVMAdapter |
| Base | ✅ ETH | ✅ EVM | Via EVMAdapter |
| Optimism | ✅ ETH | ✅ EVM | Via EVMAdapter |
| Linea | ✅ ETH | ✅ EVM | Via EVMAdapter |
| Near | ✅ NEAR | ⚠️ Planned | NearAdapter P3 |
| Cosmos | ✅ ATOM | ✅ Executor | CosmosAdapter active |
| XRP Ledger | ✅ XRP | ⚠️ Planned | XRPLAdapter P3 |
| Tron | ✅ TRX | ⚠️ Planned | TronAdapter P3 |
| Celo | ✅ CELO | ✅ EVM | Via EVMAdapter |
| Fantom | ✅ FTM | ✅ EVM | Via EVMAdapter |
| Gnosis | ✅ GNO | ✅ EVM | Via EVMAdapter |
| Stellar | ✅ XLM | ⚠️ Planned | StellarAdapter P3 |
| Sui | ✅ SUI | ⚠️ Planned | SuiAdapter P2 |
| Aptos | ✅ APT | ⚠️ Planned | AptosAdapter P2 |
| Sei | ✅ SEI | ⚠️ Planned | Via CosmosAdapter |
| Cronos | ✅ CRO | ✅ EVM | Via EVMAdapter |
| Kava | ✅ KAVA | ✅ EVM | Via EVMAdapter |
| Mantle | ✅ MNT | ✅ EVM | Via EVMAdapter |
| zkSync | ✅ ETH | ✅ EVM | Via EVMAdapter |
| Scroll | ✅ ETH | ✅ EVM | Via EVMAdapter |
| Starknet | ✅ STRK | ⚠️ Planned | StarknetAdapter P3 |
| Berachain | ✅ BERA | ✅ EVM | Via EVMAdapter |
| Monad | ✅ MONAD | ⚠️ Planned | MonadAdapter P3 |
| Taiko | ✅ TAIKO | ✅ EVM | Via EVMAdapter |
| Blast | ✅ BLAST | ✅ EVM | Via EVMAdapter |
| BaseSepolia | ✅ ETH | ✅ EVM | Testnet only |

### Protocol Module Coverage

| SDK Module | Nexus Coverage | Status |
|-----------|---------------|--------|
| bitcoin | ✅ Full | Primary chain |
| statechain | ❌ Not covered | Structural boundary in SDK (P2) |
| stacks | ✅ Executor | StacksAdapter |
| lightning | ✅ SRL-1 | LightningResilienceAdapter |
| ethereum/evm | ✅ Executor | EVMAdapter |
| cosmos | ✅ Executor | CosmosAdapter |
| bitvm | ✅ Executor | BitVMAdapter |
| rgb | ✅ Executor | RGBAdapter |
| fedimint | ✅ Executor | FedimintAdapter |
| dlc | ⚠️ API only | DLC API route |
| zkml | ⚠️ API only | ZKML API route |
| mmr | ✅ State | Core state commitment |
| solana | ⚠️ Planned | SolanaAdapter P2 |
| ark | ❌ Not covered | P3 |
| bip322 | ❌ Not covered | P3 |
| musig2 | ❌ Not covered | P3 |
| frost | ❌ Not covered | P3 |
| covenant | ❌ Not covered | P3 |
| identity | ✅ API | Identity API route |
| settlement | ✅ API | Settlement API route |
| a2p | ❌ Not covered | P3 |
| account_abstraction | ❌ Not covered | P3 |
| cctp | ❌ Not covered | P3 |
| chain_abstraction | ❌ Not covered | P3 |
| credit | ❌ Not covered | P3 |
| economy | ❌ Not covered | P3 |
| fiat | ❌ Not covered | P3 |
| intent | ❌ Not covered | P3 |
| job_card | ❌ Not covered | P3 |
| opportunity | ❌ Not covered | P3 |
| sidl | ❌ Not covered | P3 |
| solver | ❌ Not covered | P3 |
| stablecoin_orchestrator | ❌ Not covered | P3 |
| swap_router | ❌ Not covered | P3 |
| business | ❌ Not covered | N/A (business logic layer) |

### Settlement Rails (SDK → Nexus)

| Rail | SDK | Nexus | Status |
|------|-----|-------|--------|
| x402 | ✅ | ⚠️ Planned | Open payment protocol |
| Wormhole | ✅ | ⚠️ Planned | Cross-chain messaging |
| NTT | ✅ | ⚠️ Planned | Native token transfer |
| Bisq | ✅ | ❌ Not covered | P2P exchange |
| Boltz | ✅ | ❌ Not covered | Atomic swap |
| Changelly | ✅ | ❌ Not covered | Instant exchange |

## Build & Test
- **Build**: `cargo build --workspace`
- **Test**: `cargo test --workspace`
- **Docker**: `docker-compose up --build` (PostgreSQL 15 + Redis 7)
- **MSRV**: Rust 1.82+, edition 2021

## Verification Protocol
1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. Verify health endpoints: REST `/health`, gRPC health check
5. Verify MMR state root consistency

## Cross-Repo Dependencies
- **lib-conxian-core**: Shared protocol primitives (git dependency, pinned rev)
- **conxius-enclave-sdk**: Hardware enclave (optional, via lib-conxian-core `enclave` feature)

### Core Module Usage (Session 58 — Aug 2026)

Nexus consumes `lib-conxian-core` (v0.3.3, exact git rev `b85625f`) directly through three module boundaries. The legacy `compat::core_bridge::core_types` re-export shim was removed in PR #259 — Nexus now imports canonical types straight from their source modules:

| Core module | Nexus consumer | Types used |
|-------------|----------------|------------|
| control_model | `src/verification/mod.rs` | Chain, ChainFamily, RiskProfile, RiskTarget, OverallRiskStatus, RiskProfileError, canonical risk-profile set |
| verifier | `src/verification/mod.rs` | ChainId |
| enclave | `src/executor/mod.rs` | AttestationCertificate |
| deployment | `src/api/admin.rs` | VerificationResult, VerificationOutcome |

`src/compat/core_bridge.rs` is Nexus-owned secp256k1 signing + HASH160 (k256/ripemd/sha2) — it does not depend on Core.
- **conxian-gateway**: Downstream consumer of Nexus proofs
- **conxius-enclave-sdk**: SDK defines canonical chain registry — Nexus aligns observation coverage

### Session 48 — Enhancement Implementation Complete

All 7 market enhancement phases implemented. Documentation in conxian_market@39136c0:

| Doc | Covers | Relevant Nexus Role |
|-----|--------|---------------------|
| `SETTLEMENT_RAILS.md` | 6 rails cataloged | Execution attestation for Managed+ tiers |
| `monitoring.md` | sBTC, Fedimint, Babylon health | Nexus provides attestation data for metrics |
| `sla_bounty_system.md` | CJCS gap cards, reputation | Enclave attestation gates auto-bounty execution |
| `trust_tier_pricing.md` | Tier detection, SLA templates | Nexus `ExecutionRequest` carries TrustTier (PR #196) |

> Nexus enclave attestation (PR #196) is the gating mechanism for Managed/Strict tier auto-execution.
> See `conxian_market/docs/knowledge_base/trust_tier_pricing.md` §2 for tier detection flow.

## License
BUSL-1.1 (Business Source License 1.1). Change Date: 2030-01-01. Change License: GPL-3.0-or-later.
See `LICENSE` for full text. SPDX identifier: `BUSL-1.1`.

---
© 2026 Conxian Foundation. Code is Law.

## Session State (2026-08-01)

### v0.4.23 — lib-conxian-core pinned to v0.3.3 release tag
- Bumped `lib-conxian-core` git rev `134e6f48` → `b85625f` (the `v0.3.3` release commit). The direct-source policy prohibits `tag` declarations, so the pin stays an exact full-SHA `rev`.
- Dependency-policy rev synced in `scripts/check_dependency_declarations.py`, `scripts/test_check_dependency_declarations.py`, and `scripts/generate_compliance_artifacts.sh` (version `0.3.2` → `0.3.3`).

### v0.4.23 — lib-conxian-core pinned to latest main (Session 58)
- Bumped `lib-conxian-core` git rev `60e92aa` → `134e6f48` (Session 58: full non-SDK capacity, public `validate_evidence_binding`, `core_types`/`compat` canonical re-exports).
- Graph stays yanked-crate-free (no secp256k1 0.32.0-beta.2 / bitcoin 0.33.0-beta) and carries no `full-sdk`/`conxius-enclave-sdk` tree.
- Dependency-policy rev synced in `scripts/check_dependency_declarations.py`, `scripts/test_check_dependency_declarations.py`, and `scripts/generate_compliance_artifacts.sh`.

### v0.4.23 — Session 48: Billing Tiers + CI Pass
- PR [#203](https://github.com/Conxian/conxian-nexus/pull/203) merged: paid tiers with LN upgrade flow (CON-24)
- PR [#200](https://github.com/Conxian/conxian-nexus/pull/200) merged: BitVM Groth16 research salvage (CON-1533)
- CI all green: Build & Test, Hygiene, Contamination guard
- SubscriptionTier: Free, Developer, Professional, Enterprise via `src/api/billing/`

### v0.4.22 — lib-conxian-core v0.3.0 Dependency
- PR [#189](https://github.com/Conxian/conxian-nexus/pull/189) merged to main
- `Cargo.toml`: added `lib-conxian-core` git dependency
- `src/compat/core_bridge.rs`: new `core_types` sub-module re-exporting:
  - Chain, ChainFamily, BridgeSystem, TrustTier, VerificationClass, FinalityClass
  - ProtocolVerifier, ProofVerificationRequest/Result, TransactionFinalityStatus
  - SigningTarget, SigningAlgorithm, SignerCapabilities
- Existing tag `v0.4.22` preserved (version unchanged)
- Dependency review config added to allow Core license
