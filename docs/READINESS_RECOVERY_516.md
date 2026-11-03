# Nexus readiness recovery — issue #516

The 3 November 2026 deployed logs report an external height near 9.4 million
and local height zero. The previous safety monitor read a Stacks block height
from `/extended/v1/block`, although it documented a Bitcoin burn-height check.
Its SQL maximum also included every block type and state.

The repair reuses `StacksBitcoinHeightProvider` and `/v2/info`'s
`burn_block_height`, rejects unsuccessful HTTP responses, and selects only
positive `burn_block` / `hard` local observations. Startup and failed checks
assert safety mode; a successful comparison is required to clear it.

This does not restore ingestion. `NexusSync::load_initial_state` is empty and
the current WebSocket reader does not process messages. Do not populate the
local height with a fetched tip just to pass readiness. Complete a separately
reviewed canonical ingestion/reorg/replay and durable-state recovery contract
before claiming a processed observation or proof journey.

The deployed Redis sidecar logs an unsafe-Redis override. Qualifying the
authenticated durable backend, PostgreSQL replay acceptance, recovery and
independent operational release review remains required. No flag is cleared
manually, and no production database or provider configuration is changed.

Validation: `cargo test --locked --test burn_height_provider_test` exercises
the HTTP contract with actual local HTTP responses, including conflicting
Stacks/Bitcoin heights, unsuccessful status, missing/zero/negative/string
heights and malformed JSON. These fixtures are regression tests, not mainnet
or production acceptance evidence.
