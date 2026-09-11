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
FROST/ROAST, ZKCP, OP_CAT). The remaining work is **scope + proof-surface +
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
| **P0** | **TEE remote-attestation depth** — Nexus X.509 `not_before/not_after` + root-of-trust checks (Hole 2.1) are insufficient for production signer gating without Android StrongBox / AWS Nitro attestation roots, revocation, and distributed-replay defense. | enclave-sdk #240/#241/#242 (P0) + #202 (release acceptance) | Research: remote-attestation evidence model, key-attestation (`KeyMint`/StrongBox) ↔ enclave measurement binding. |
| **P0** | **Curve / verifier-ownership contract** — Nexus verifies on Arkworks/BLS12-381; Gateway exposes a BN254 Groth16 envelope. No single curve/VK/public-input/state-root/verifier-ownership contract exists. | Gateway #189 (G-2) | Research: pick one canonical proof surface and a cross-repo verifier-ownership boundary. |
| **P1** | **Settlement-rail enablement** — `x402`, `Wormhole`, and `NTT` are "Planned"; `Bisq`/`Boltz`/`Changelly` are not covered. | — | Research: x402 V2 settlement lane (see dated finding below), NTT relayer handoff, Wormhole observation. |
| **P1** | **DLC + ZKML full verification** — `dlc` and `zkml` routes are API-only (no oracle-signature / CET / real ZKML proof verification). | — | Research: DLC oracle attestation + CET construction; ZKML proving-key pinning. |
| **P1** | **IdempotencyStore → Neon + live-DB conformance** | #251 | Engineering + conformance suite (not pure research, but a release gate). |
| **P1** | **Chain coverage P2** — Solana, Sui, Aptos adapter specifications. | — | Research: consensus/state-proof formats for each. |
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
