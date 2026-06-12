# ADR-006: Tier 1 Chain Families for Nexus and Gateway Execution

## Status
Proposed (2026-06-12)

## Context
Conxian Nexus (Glass Node) and Conxian Gateway require a prioritized list of chain families to implement for initial production execution. The "adapter-family" architecture allows for broad support, but focus is needed to prevent implementation drift and ensure high-tier reliability for the most strategic ecosystems.

## Decision
We will prioritize the following three chain families as **Tier 1** for the initial rollout of Conxian Nexus and Gateway:

1.  **Bitcoin / UTXO Family**
    *   **Rationale**: Foundation of Conxian's sovereignty-first ethos. Direct integration with Stacks (L1), RGB, and DLCs. Required for non-custodial Bitcoin orchestration.
    *   **Priority**: P0

2.  **Ethereum / EVM Family**
    *   **Rationale**: Industry standard for institutional DeFi and asset issuance. High integration demand from B2B partners. Well-defined state-root and receipt proof models for Glass Node monitoring.
    *   **Priority**: P1

3.  **Cosmos / IBC Family**
    *   **Rationale**: Superior interoperability model (IBC) and modular architecture. Fits the Conxian adapter-family design. Enables connection to a vast ecosystem of sovereign application-specific chains.
    *   **Priority**: P1

### Deferred Families (Tier 2/3)
*   **Solana / SVM**: Deferred due to higher runtime complexity in transaction proof verification and a distinct non-UTXO/non-EVM state model.
*   **Move (Sui/Aptos)**: Deferred pending further institutional demand.
*   **Substrate / Polkadot**: Deferred; to be revisited once XCM integration requirements are formalized.

## Implementation Implications

### Nexus (Glass Node)
*   **Bitcoin**: Native Stacks sync is already optimized. Expand to include full UTXO state monitoring for local DLC/RGB validation.
*   **EVM**: Implement JSON-RPC state-root polling and receipt proof verification logic.
*   **Cosmos**: Implement IBC Light Client verification within the Nexus state layer.

### Gateway
*   **Transport Adapters**: Prioritize Bitcoin (PSBT/Lightning), EVM (JSON-RPC), and Cosmos (gRPC/Protobuf) transport layers.
*   **Policy Enforcement**: Trust-tier decisions will first be hard-coded for these three families.

## Conformance
All future adapters must inherit from the family-specific base classes defined in `lib-conxian-core`.

## References
*   [CON-789](https://linear.app/conxian-labs/issue/CON-789)
*   [PRD.md](./PRD.md)
