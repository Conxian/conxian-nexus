# Lightning Resilience and Recovery Layer (SRL-1)

## Overview
This document defines the operational resilience layer for Lightning Network payments within the Conxian ecosystem. It addresses the unique failure modes of off-chain payments and provides a formalized framework for recovery and status tracking.

## Failure Taxonomy
We categorize Lightning payment failures into three primary types to guide automated and manual recovery efforts:

1.  **Permanent**
    *   **Description**: Failures that cannot be recovered by retrying the same payment intent.
    *   **Examples**: `no_route`, `invalid_invoice`, `amount_too_small`, `final_incorrect_cltv_expiry`.
    *   **Action**: Mark payment as Failed and notify the user/BFF. Do not retry.

2.  **Transient**
    *   **Description**: Temporary failures that may resolve on a subsequent attempt or after a short delay.
    *   **Examples**: `temporary_node_failure`, `temporary_channel_failure`, `peer_disconnected`.
    *   **Action**: Initiate automated retry logic or move to `Recovering` state.

3.  **Indeterminate**
    *   **Description**: States where the final outcome of the payment is unknown (e.g., payment in flight).
    *   **Examples**: `timeout`, `mpp_timeout`.
    *   **Action**: Enter a monitoring loop. Wait for proof of success or definitive failure from the channel manager or watcher.

## Payment Lifecycle State Machine
Payments move through the following states, validated by the `LightningResilienceAdapter`:

*   **Pending**: Payment intent created, not yet sent.
*   **Succeeded**: Preimage received and funds settled.
*   **Failed**: Permanent failure reached.
*   **Recovering**: Transient or Indeterminate failure triggered a recovery attempt.

### Valid Transitions
*   Pending -> Succeeded | Failed | Recovering
*   Recovering -> Succeeded | Failed
*   Failed -> Recovering (Manual intervention or re-scan)

## Implementation
The logic is encapsulated in `src/executor/lightning.rs` and integrated into the `NexusExecutor`.

## Guardrails
*   Conxian does **not** take possession of customer funds.
*   The resilience layer strengthens non-custodial execution by automating the "stuck payment" monitoring.
*   Sensitive customer data (like private preimages for large payments) should be handled in the secure enclave, not stored long-term in the Nexus persistence layer.

## References
*   [CON-688](https://linear.app/conxian-labs/issue/CON-688)
*   [CON-1174](https://linear.app/conxian-labs/issue/CON-1174)
