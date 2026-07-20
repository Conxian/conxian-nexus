# Universal Chain Support Boundaries

## Overview
This document defines the architectural boundaries for multi-chain support across the Conxian stack. It ensures a clear separation between the protocol core and the edge adapters.

## Architecture

1.  **Protocol Core (Narrow & Opinionated)**
    *   **Focus**: Stacks L1 and Bitcoin (via Stacks).
    *   **Responsibilities**: Hard-finality anchoring, canonical state root commitment, and core governance.
    *   **Boundary**: The protocol core should only expand to other chains if they offer comparable sovereignty and hard-finality guarantees (e.g., L2s with L1-verified fraud/validity proofs).

2.  **Glass Node (Nexus)**
    *   **Focus**: Tier 1 Chain Families (Bitcoin, EVM, Cosmos).
    *   **Responsibilities**: Cross-chain state monitoring, proof verification, and event normalization.
    *   **Boundary**: Nexus uses family-specific adapters to monitor state without modifying the target chain.
    *   **Bitcoin limitation**: Nexus does not currently provide a native Bitcoin full-node, SPV, compact-filter, or UTXO observation backend. Bitcoin references in this repository must not be read as live Bitcoin synchronization. The Phase 1 BIP-110 alignment is a pure assessment of caller-supplied size metadata only.

3.  **Fusion Gateway**
    *   **Focus**: Universal connectivity.
    *   **Responsibilities**: Transport adaptation, PSBT/Transaction construction, and institutional API bridging.
    *   **Boundary**: Gateway handles the "messy" reality of multiple RPCs and networking protocols, keeping the Nexus and Core clean.

## Multi-Chain Expansion Criteria
Deep protocol-core expansion to a new chain is justified only if:
1.  **Sovereignty**: The chain supports non-custodial, peer-to-peer asset control.
2.  **Proof Model**: The chain provides compact, verifiable state roots or transaction receipts.
3.  **Institutional Demand**: Significant B2B or community interest exists for native settlement on that chain.

## References
*   [CON-810](https://linear.app/conxian-labs/issue/CON-810)
*   [ADR-006: Tier 1 Chain Families](./ADR-006_Tier1_Chain_Families.md)
