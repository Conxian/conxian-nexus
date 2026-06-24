# Conxian Nexus Research & Improvement Proposals

## 1. Multi-Chain Interoperability
- **Cosmos/IBC**: Researching "superior" interoperability models for cross-chain state proofs.
  - *Reference*: [IBC Protocol Specification](https://github.com/cosmos/ibc)
- **BitVM2**: Optimistic bridge research for trust-minimized Bitcoin L2s.
  - *Reference*: [BitVM: Compute Anything on Bitcoin](https://bitvm.org/bitvm.pdf)

## 2. Smart Contract Language Evolution
- **Clarity 4**: Transitioning to passkey-based auth (`secp256r1-verify`) and on-chain contract hashes (`contract-hash?`).
  - *Reference*: [Stacks 2.5/3.0 SIPs](https://github.com/stacksgov/sips)

## 3. Sovereign Persistence
- **Tableland**: Decentralized relational storage for immutable audit trails.
  - *SDK*: `@tableland/sdk`
- **Kwil**: Sovereign SQL for high-performance off-chain state commitments.
  - *SDK*: `kwil-js` / `kwil-rust`

## 4. Improvement Proposals (Nexus-Specific)
- **NIP-01**: Transition Admin API to dual-signature requirement for release approval.
- **NIP-02**: Implement ZKML verification for oracle confidence scoring.
- **NIP-03**: Expand SRL-1 Lightning Resilience to include MPP (Multi-Path Payment) recovery.

## 6. Emerging Research Areas (CON-1302, CON-1303, CON-1304)

### 6.1 FROST Threshold Signatures (CON-1302)
- **Concept**: Flexible Round-Optimized Schnorr Threshold Signatures (FROST) allows for threshold signatures that result in standard Schnorr signatures.
- **Application**: Multi-sig vaults that are indistinguishable from single-sig on-chain, reducing fees and increasing privacy.
- **Resources**: Research `frost-dalek` and `roast` for implementation in `lib-conxian-core`.

### 6.2 OP_CAT Recursive Covenants (CON-1303)
- **Concept**: BIP-347 proposes restoring the `OP_CAT` opcode to Bitcoin, enabling stack item concatenation.
- **Application**: Enables recursive covenants, CAT-SMT (Merkle Trees in Script), and complex vault structures without hard forks.
- **Nexus Role**: Monitor OP_CAT-enabled spending conditions in the Glass Node for advanced Bitcoin L2 scaling.

### 6.3 Fedimint Community Liquidity (CON-1304)
- **Concept**: Fedimint is a protocol for federated Chaumian Mints.
- **Application**: Community-governed liquidity and privacy-preserving e-cash.
- **Integration**: A "Federation Adapter" in Nexus to synchronize mint state and verify e-cash issuance/redemption proofs.
