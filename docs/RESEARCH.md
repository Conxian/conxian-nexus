# Conxian Nexus Research & Improvement Proposals (Updated August 2026 - v0.4.23)

## 1. Multi-Chain Interoperability (NIP-005)

### 1.1 Bitcoin & BitVM2
- **Concept**: Optimistic bridge research for trust-minimized Bitcoin L2s and state transition verification.
- **Status**: The canonical BN254 boundary uses `ark-groth16` in `bitvm_groth16`, composed by `canonical_bitvm` for trusted-key lookup, height validation, audit persistence, and HTTP handling. Real cryptographic verification of Groth16 SNARK proofs against verified verifying keys is active.

### 1.2 Cosmos & IBC Header Verification
- **Concept**: Trust-minimized cross-chain state proofs using the Inter-Blockchain Communication (IBC) protocol and Tendermint header validation.
- **Implementation Path**: Decodes base64 Tendermint client update headers, computes SHA-256 header payload digests, and enforces strict block height progression (`latest_height > trusted_height`) with persistent audit logging in `cosmos_verified_client_updates`.
- **Status**: **Upgraded to Cryptographic Verification (v0.4.23)** in `src/executor/cosmos.rs`.

### 1.3 EVM Merkle Patricia Trie (MPT) Receipt Proof Verification
- **Concept**: Verifying that a transaction receipt belongs to a specific block's receipt root via Merkle Patricia Trie (MPT) node hash chain verification.
- **Implementation Path**: Parses hex-encoded proof nodes and receipt roots, verifies that node 0 Keccak-256 hash equals `receipt_root`, verifies parent-child hash linkages across the branch, and persists audit state in `evm_verified_receipts`.
- **Status**: **Upgraded to Cryptographic Verification (v0.4.23)** in `src/executor/evm.rs`.

### 1.4 Stacks & sBTC Integration (CON-1200)
- **Concept**: Stacks 2.5/3.0 SIP alignment with passkey-based WebAuthn SECP256R1 authentication, contract bytecode hash verification, and sBTC peg-in/peg-out transaction verification.
- **Implementation Path**:
  - **Phase 1**: Structural validation of transaction IDs and positive sBTC amounts.
  - **Phase 2 (v0.4.23 Upgrade)**:
    1. Cryptographic validation of Stacks mainnet (`SP`) and testnet (`ST`) c32/bech32 address prefixes and length constraints.
    2. Strict 0x-prefixed 32-byte transaction ID format verification.
    3. Positive sBTC satoshi amount bounds and valid block height enforcement.
    4. SQLx PostgreSQL persistence in `stacks_verified_transactions` table with duplicate transaction detection and immutable audit logging.
- **Status**: **Upgraded to Phase 2 Cryptographic Audit & Database Persistence (v0.4.23)** in `src/executor/stacks.rs`.

## 2. Admin & Governance Hardening

### 2.1 Cryptographic Dual-Signatures (NIP-004)
- **Status**: **COMPLETED v0.4.17**. Secp256k1 signature verification active for all write/governance endpoints using `k256`.

### 2.2 Admin Token Hardening (NIP-006)
- **Status**: **COMPLETED v0.4.18**. Replaced static bearer token with a scoped credential pool (API Keys) issued via Dual-Signature login (`/admin/v1/login`). Scoped keys are prioritized; static fallback is restricted and flagged in production.

## 3. Resilience & Failure Modes

### 3.1 SRL-1 Recovery Triggers (Hole 3.1)
- **Status**: **COMPLETED v0.4.18**. Automatic recovery actions (Retry for transient errors, Split-Recovery for MPP failures, Reconciliation for indeterminate states) active via `AutonomousOrchestrator`.

## 4. Smart Contract Language & Enclave Evolution

### 4.1 Clarity 4 & Stacks Integration (CON-1200)
- **Concept**: Stacks Clarity 4 contract verification and sBTC threshold transaction consensus.
- **Status**: **Phase 2 Cryptographic Audit & Persistence Active** in `src/executor/stacks.rs`.

### 4.2 Hardware Enclave Attestation Verification (Hole 2.1)
- **Concept**: Hardware-backed X.509 attestation certificate verification for confidential execution requests originating from TEE enclaves.
- **Specification & Design**:
  1. Parse X.509 DER certificates submitted with `ExecutionRequest` payloads.
  2. Verify attestation certificate chain against hardware root-of-trust (Intel SGX / AMD SEV-SNP).
  3. Validate enclave measurement hashes against authorized workload measurements.
  4. Enforce strict certificate validity window checks (`not_before` / `not_after`).
- **Status**: **Upgraded to X.509 DER Cryptographic Verification (v0.4.23)** in `src/executor/mod.rs`. Decodes raw DER payloads using `x509-cert`, verifies certificate validity windows (`not_before` / `not_after`), and enforces strict attestation checks when `require_attestation` is set.

## 5. Sovereign Persistence & Storage Boundaries
- **Hole 1.2 (Redis Auth)**: **COMPLETED v0.4.18**. Enforced authenticated remote Redis connections in production release builds.
- **Tableland & Kwil**: Decentralized relational storage adapters for audit trails, state commitments, and sovereign OLTP persistence.

## 6. Emerging Research Areas (CON-1302, CON-1303, CON-1304, CON-1313)

### 6.1 Zero-Knowledge Contingent Payments (CON-1313 / G-50)
- **Concept**: Fair exchange of digital goods and secrets against Bitcoin/Lightning payments using SNARK SHA-256 pre-image circuit verification.
- **Cryptographic Pipeline**:
  1. Seller constructs a SHA-256 pre-image circuit using `ark-groth16` proving key.
  2. Buyer verifies Groth16 SNARK proof that hash `H(s) = Y` matches the payment HTLC hash condition.
  3. Upon payment settlement on Bitcoin/Lightning, the secret pre-image `s` is extracted from the transaction input.
- **Status**: Scaffolding in `lib-conxian-core`.

### 6.2 FROST Threshold Signatures (CON-1302)
- **Concept**: Flexible Round-Optimized Schnorr Threshold Signatures for Taproot multi-party orchestration without revealing threshold policy structure on-chain.
- **Protocol Specification**:
  1. Two-round threshold signing protocol generating standard BIP-340 Schnorr signatures.
  2. Integrates with ROAST (Robust Threshold Schnorr) orchestrator (`src/orchestrator/roast.rs`) for fault-tolerant participant set management:
     - **Round 1 (Commitments)**: Collect participant nonce commitments within `commit_timeout`.
     - **Filter & Exclude**: Exclude timed-out or faulty participants while verifying active candidates count >= `threshold`.
     - **Round 2 (Shares & Aggregation)**: Dispatch signing package to cooperative subset and aggregate signature shares.
     - **Fault Isolation**: Flag faulty nodes persistently across rounds; allow timed-out nodes to rejoin on round retries up to `max_retries`.
  3. Indistinguishable on-chain from single-key Taproot key-path spending.
- **Status**: **Orchestrator Integrated (v0.4.23)** via ROAST orchestrator in `src/orchestrator/roast.rs`.

### 6.3 OP_CAT Recursive Covenants (CON-1303 / BIP-347)
- **Concept**: Taproot script execution with OP_CAT covenant tree verification for vault spending restrictions and recursive contract state machines.
- **Execution Model**:
  1. Concatenates script elements using OP_CAT to construct transaction introspective checks.
  2. Validates output scripts and transaction hash structures against predefined vault policies.
  3. Enforces locktimes and recipient whitelist covenants on Bitcoin L1.
- **Status**: Active research and execution simulation.

### 6.4 Fedimint Community Liquidity & e-Cash Verification (CON-1304)
- **Concept**: Federated blind signatures issuing untraceable e-cash for community privacy pools.
- **Status**: **Phase 2 Cryptographic Audit Completed (v0.4.23)** in `src/executor/fedimint.rs`. Verifies blinded mint proofs, derives SHA-256 nonce digests, checks double-spending against `fedimint_verified_proofs` in SQLx, and logs immutable audit records.

## 7. Production Alignment & Settlement Infrastructure (v0.4.23)

### 7.1 Lightning Billing Settlement & BOLT11 Encodings (CON-24)
- **Architecture**: B2B Subscription tier upgrades (`/api/billing/upgrade` and `/api/billing/verify-payment`) generate standard canonical `lnbc` BOLT11 payment requests.
- **Verification Path**: Pending upgrade invoices map `invoice_id -> (api_key, target_tier, amount_sats)` inside Redis with 3600s TTL. Payment verification inspects persistent settlement state and migrates the API key tier (`apikey:<api_key>` field `tier`).

### 7.2 gRPC Production Authorization & Storage Validation
- **Architecture**: gRPC authentication via `grpc_auth_interceptor` and `NexusGrpcService::check_auth`.
- **Enforcement Path**: Rejects unauthenticated metadata, enforces key length bounds (>= 16 chars), fails closed on Redis connection drops, and performs production credential lookup against Redis (`apikey:<api_key>`).

## 8. Best Candidate Initialization Specifications

### 8.1 Candidate 1: FROST Threshold Signature Productionization (CON-1302)
- **Primary Domain**: Schnorr Taproot Threshold Signing (`src/orchestrator/roast.rs`)
- **Impact Score**: 9/10
- **Effort Score**: 6/10
- **Candidate Status**: **Orchestrator Integrated (v0.4.23)**
- **Architecture & Implementation Matrix**:
  1. **ROAST Engine Integration**: Connects `RoastConfig` with `FrostSigningContext` to orchestrate 2-round Schnorr signing with dynamic participant subset filtering.
  2. **Fault Exclusion**: Identifies and isolates malicious or slow signers across rounds, persisting fault metrics.
  3. **Taproot On-Chain Compatibility**: Outputs standard BIP-340 Schnorr signatures indistinguishable from single-key outputs.

### 8.2 Candidate 2: ZKCP Pre-Image Circuit Verification (CON-1313 / G-50)
- **Primary Domain**: Zero-Knowledge Contingent Payments (`lib-conxian-core`)
- **Impact Score**: 8/10
- **Effort Score**: 7/10
- **Candidate Status**: **Secondary Candidate Initialized**
- **Architecture & Implementation Matrix**:
  1. **SHA-256 Circuit Pipeline**: Validates Groth16 SNARK proofs for preimage verification without secret disclosure prior to payment execution.
  2. **Atomic Settlement Gate**: Verifies HTLC secret revelation on Bitcoin/Lightning upon settlement.
