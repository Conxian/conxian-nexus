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
