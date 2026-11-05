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

## Canonical ingestion (follow-up)

`src/sync/chain_ingest.rs` now performs the canonical ingestion the previous
patch deliberately deferred. On startup and every 30 seconds the sync service
reads `/extended/v1/block`, derives each committed anchor block's Bitcoin burn
block (`burn_block_hash` + `burn_block_height`), and persists the distinct burn
blocks into `stacks_blocks` as `type = 'burn_block'`, `state = 'hard'`. The
safety comparison therefore sees a real positive processed burn height that
tracks `/v2/info` `burn_block_height`, rather than an empty table.

Reorg handling: when a new canonical burn block replaces one already recorded
at the same height, the old row is marked `orphaned` in the same transaction
that inserts the replacement, so two `hard` burn blocks can never share a
height. A non-canonical block or a row without a burn identity is skipped, never
invented. An unavailable or malformed indexer response is an error, not an empty
success; safety mode stays asserted until a real observation lands.

This is observation-only: it signs nothing, submits no transaction, and mutates
no provider configuration. It does not yet provide full historical backfill,
proof-of-work or Merkle verification, durable Redis binding, or independent
operational acceptance, which remain open under #516 and #439.
