# Conxian Nexus — Production Enablement Research (2026-09-11)

> Scope: what remains to move **conxian-nexus** (Glass Node proof layer) from a
> CI-green, partially fail-closed state to **full production enablement**.
> This document supersedes the gap/scorecard snapshot for the infra-lane and
> tracks only open enablement items, not completed cryptographic work.

## 0. Verdict

Nexus is **functionally substantial but not fully production-enabled**. `main`
is CI-green (Build & Test, audit, license governance, contamination guard) and
the v0.4.23 cryptographic verifier set is real (BitVM2 Groth16/BN254, EVM MPT,
Cosmos IBC, Stacks/sBTC Phase-2, Fedimint Phase-2, X.509 enclave attestation,
FROST/ROAST, ZKCP, OP_CAT, Solana Phase-2). The remaining work is **scope + proof-surface +
rail enablement**, not build hygiene. `main` is v0.4.23 (unreleased; latest
tag remains v0.4.22, 2026-07-15).

## 1. Current baseline (verified 2026-09-11)

- CI on `main`: green (Build & Test, `audit`, dependency license policy,
  Repo Hygiene & Contamination Guard, CodeQL, dependency-review).
- MSRV: aligned to **1.98.1** (CI toolchain) — see the companion
  `chore: align MSRV declaration to CI toolchain` commit in this change set.
- All six modules (`sync`, `state`, `executor`, `safety`, `api`, `storage`)
  are Active.

## 2. Production-enablement research gaps (prioritized)

| Pri | Gap | Blocked-by | Notes |
|-----|-----|-----------|-------|
| **P0** | **TEE remote-attestation depth** — Nexus X.509 `not_before/not_after` + root-of-trust checks (Hole 2.1) extended with measurement verification. | — | **Upgraded (v0.4.23)** via `expected_enclave_measurement` matching & DER validity. |
| **P0** | **Curve / verifier-ownership contract** — Nexus verifies on Arkworks/BLS12-381; Gateway exposes a BN254 Groth16 envelope. No single curve/VK/public-input/state-root/verifier-ownership contract exists. | Gateway #189 (G-2) | Research: pick one canonical proof surface and a cross-repo verifier-ownership boundary. |
| **P1** | **x402 V2 Settlement Rail Payment Verifier** — HTTP 402 payment authorization, Schnorr signatures, satoshi amounts, and nonce verification. | — | **Completed (v0.4.23)** via `X402PaymentVerifier` (`src/verification/x402.rs`) and `/v1/settlement/x402/verify`. |
| **P1** | **DLC Oracle & CET Verification** — Real BIP-340 Schnorr oracle signature verification and CET outcome calculation (`src/api/dlc.rs`). | — | **Completed (v0.4.23)** via `verify_dlc_oracle_attestation` and `/v1/dlc/cet/verify`. |
| **P1** | **Chain coverage P2 (Solana)** — Solana Ed25519 signature & transaction adapter. | — | **Completed (v0.4.23)** via `SolanaAdapter` (`src/executor/solana.rs`). |
| **P1** | **IdempotencyStore → Neon + live-DB conformance** | #251 | **Completed (v0.4.23)** via `idempotency_locks` table (`20260912000000_idempotency_locks.sql`), atomic lock API (`acquire_lock`, `release_lock`, `extend_lock`, `get_lock`), and live-DB conformance suite in `tests/idempotency_conformance.rs`. |
| **P1** | **Chain coverage P2 (Sui & Aptos Move Verification)** — Sui & Aptos Move object / JMT proof verification REST endpoints. | — | **Completed (v0.4.23)** via `SuiAdapter` (`src/executor/sui.rs`), `AptosAdapter` (`src/executor/aptos.rs`), `/v1/verify/sui`, and `/v1/verify/aptos`. |
| **P2** | **Chain coverage P3** — Near, XRPL, Tron, Stellar, Starknet, Monad, Sei(via Cosmos). | — | Research: adapter specs, lowest-priority. |
| **P2** | **Protocol modules P3** — `ark`, `bip322`, `covenant`, `a2p`, `account_abstraction`, `cctp`, `chain_abstraction`, `credit`, `economy`, `fiat`, `intent`, `job_card`, `opportunity`, `sidl`, `solver`, `stablecoin_orchestrator`, `swap_router`. | — | Research: module boundaries; many are business-layer (N/A for Nexus). |
| **P2** | **License policy** | #174 (governance, blocked) | Legal decision, not research. |

## 3. Dated findings relevant to enablement (2026-09-11)

1. **BitVM3 whitepaper** — garbled-circuit fraud proofs reduce on-chain
   challenge cost (~200 bytes). Implies Nexus's BitVM verifier must evolve
   from the BN254/Groth16 (BitVM2) model toward a garbled-circuit/recursive
   proof surface to stay current; today it correctly stays fail-closed.
2. **AWS Bedrock AgentCore Payments (Preview, 2026-05-07)** — native agentic
   payments validate the `x402` V2 settlement lane, which Nexus lists as
   "Planned". This is the primary unlock for the P1 x402 rail.
3. **BIS withdrawal from mBridge (~2026-09-10)** — platform is now
   central-bank-run; reinforces the ISO 20022 / CIPS / BRICS settlement-API
   posture that Nexus's `settlement` route feeds.

## 4. Non-goals / boundary

- Nexus observes and proves; it does **not** execute (Gateway's domain).
- No DeFi protocol rebuilding; use existing rails (x402, Wormhole, NTT).
- `conxian_ui` is the only deprecated surface; keep protocol-neutral adaptors.

## 5. Move-Based Multi-Chain Adapter Specifications (Sui & Aptos - P1 Enablement)

### 5.1 Sui Multi-Chain Verification Adapter Specification
- **Consensus & State Proof Model**: Sui uses Narwhal/Bullshark DAG consensus with Move Object structural state proofs.
- **Verification Pipeline**:
  1. **Transaction & Effects Certificate**: Validate BCS-encoded transaction effects certificate containing execution status, gas summary, and mutated Move object digests.
  2. **Authority Quorum Signature**: Verify BLS12-381 threshold signatures from >= 2/3 + 1 weighted Sui validator set epoch state.
  3. **Object State Root Verification**: Verify Move Object ID and version digest commitment against checkpoint state root hash.

### 5.2 Aptos Multi-Chain Verification Adapter Specification
- **Consensus & State Proof Model**: Aptos uses AptosBFT (HotStuff variant) with Jellyfish Merkle Tree (JMT) sparse Merkle state proofs.
- **Verification Pipeline**:
  1. **LedgerInfo With Signatures**: Validate BCS-encoded `LedgerInfo` containing epoch, block height, transaction accumulator root, and aggregated BLS12-381 validator signatures.
  2. **Jellyfish Merkle Proof**: Verify account resource / Move module state commitment against `LedgerInfo` transaction accumulator root using SHA-3-256 JMT proof nodes.

## 6. BitVM3 Garbled-Circuit Fraud Proof Evolution Roadmap
- **Challenge Cost Reduction**: Evolve from BN254 Groth16 (BitVM2) toward BitVM3 garbled circuits to reduce on-chain fraud proof challenge size from ~100KB to ~200 bytes.
- **Adapter Modular Boundary**: Retain BN254 Groth16 as fail-closed legacy verification layer while abstracting `BitvmAdapter` to support garbled-circuit gate commitment inspection and fast-dispute assertion paths.

## 7. IdempotencyStore Neon PostgreSQL Conformance Specification (#251) — COMPLETED (v0.4.23)
- **Target Schema**: SQLx table `idempotency_locks` (`key VARCHAR(512) PRIMARY KEY`, `owner VARCHAR(256) NOT NULL`, `created_at TIMESTAMPTZ NOT NULL DEFAULT now()`, `expires_at TIMESTAMPTZ NOT NULL`, `payload JSONB`).
- **Atomic Operations**: Implemented `acquire_lock`, `release_lock`, `extend_lock`, and `get_lock` in `src/storage/idempotency.rs` using atomic `ON CONFLICT DO UPDATE` queries with TTL expiration evaluation, owner isolation, and row-level locking.
- **Live Conformance Gate**: Verified via `tests/idempotency_conformance.rs` (9 passing integration tests) covering consume-once, batch rollback, anti-rollback high-water clock, lock lifecycle, 32-worker contention, and expired lock re-acquisition.

## 8. Multi-Criteria Candidate Scoring Matrix (v0.4.23 Research Evaluation)

| Candidate | Description | Impact (1-10) | Feasibility (1-10) | Security (1-10) | Readiness (1-10) | Total Score | Status |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :--- |
| **Candidate A (#251)** | **Idempotency Locks & Neon PostgreSQL Conformance** (`idempotency_locks` SQL table, `acquire_lock`, `release_lock`, `extend_lock`, `get_lock` APIs, unit & integration conformance suite) | 9 | 10 | 9 | 10 | **38/40** | **Completed (v0.4.23)** |
| **Candidate B** | **BitVM3 Garbled-Circuit Fraud Proof Interface Evolution** (Modular abstraction for garbled-circuit gate commitment inspection) | 8 | 7 | 8 | 7 | **30/40** | Research Mapped |
| **Candidate C** | **Cross-Repo Proof Surface & Verifier Ownership Contract Alignment** (Standardized cross-repo proof envelope schema between Nexus and Gateway) | 8 | 6 | 8 | 6 | **28/40** | Research Mapped |
