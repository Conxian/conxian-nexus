# NIP-007: Safety Mode Enforcement in Execution Path

## Status
Proposed (2026-06-21)

## Context
Conxian Nexus implements a "Safety Mode" (`src/safety/mod.rs`) triggered by sync drift or gateway telemetry spikes. However, the primary transaction submission path (`src/executor/mod.rs`) does not currently check this global safety flag, allowing executions to proceed even when the system is in an unverified or high-drift state.

## Proposal
1.  **Fail-Closed Execution**: Update `NexusExecutor::submit` to check if Safety Mode is active via `is_safety_mode_active`.
2.  **Graceful Rejection**: If Safety Mode is active, reject transaction submission with a specific error code (`503 Service Unavailable`) and a message indicating the system is in "Sovereign Handoff" or "Recovery" mode.
3.  **Bypass Rule**: Define specific "Emergency Exit" or "System Internal" transactions that are allowed to bypass the safety check (if applicable).

## Consequences
- **Resilience**: Prevents state corruption during critical system drift.
- **Availability**: Transactions will be rejected during safety triggers, but the system's integrity remains protected.

## References
- [HOLE_REPORT.md](./remediation/HOLE_REPORT.md)
- [LIGHTNING_RESILIENCE_LAYER.md](./LIGHTNING_RESILIENCE_LAYER.md)
