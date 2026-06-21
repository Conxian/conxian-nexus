# NIP-005: Transition from Simulated to Real Multi-Chain Verification

## Status
Proposed (2026-06-21)

## Context
Conxian Nexus (Glass Node) currently uses "simulated verification floors" for Tier 1 Chain Families (Bitcoin/BitVM2, EVM, Cosmos). While this allows for architectural testing, it does not provide the cryptographic guarantees required for a production-intent Glass Node.

## Proposal
Implement real cryptographic verification for the following adapters:

### 1. BitVM2 (Bitcoin/UTXO)
- **Current**: Validates state root format and logs to DB.
- **Target**: Integrate a STARK/SNARK verifier (e.g., using `arkworks` or `halo2`) to verify the `proof_bytes` against the state roots.
- **Phase 1**: Add a "Validation Tier" to the response (e.g., `Tier: Simulated` vs `Tier: Cryptographic`).

### 2. EVM (Ethereum)
- **Current**: Checks block hash and receipt root format.
- **Target**: Implement Merkle Patricia Trie (MPT) verification logic to prove a transaction receipt belongs to a block's receipt root.
- **Dependency**: Add `eth-trie` or equivalent crate.

### 3. Cosmos (IBC)
- **Current**: Validates `client_id` format.
- **Target**: Implement Tendermint Light Client verification (header validation, signature checking against validator sets).

## Consequences
- **Integrity**: Nexus becomes an authoritative source of truth for cross-chain state.
- **Performance**: Cryptographic verification is CPU-intensive; may require asynchronous worker pools.

## References
- [ADR-006](./ADR-006_Tier1_Chain_Families.md)
- [HOLE_REPORT.md](./remediation/HOLE_REPORT.md)
