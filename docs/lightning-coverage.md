# Lightning Coverage Gate

Nexus enforces Lightning-focused coverage with `scripts/check_lightning_coverage.py`.

## Why this is line-range scoped (not full-file)

`cargo llvm-cov` built-in fail-under thresholds are file/workspace-wide. Our Lightning code paths live in mixed files (`src/api/rest.rs` also contains many unrelated endpoints), so file-level thresholds would fail on unrelated low-coverage areas and create noisy regressions.

To keep the gate accurate and actionable, we enforce line coverage on Lightning-specific ranges:

- `src/api/billing/mod.rs`
  - billing route wiring
  - grace/auth/quota policy helpers used by `track_signature`
- `src/api/billing/nostr.rs`
  - collector filtering, key parsing, dedup/bridge decision helpers
- `src/api/dlc.rs`
  - request validation and announcement/signing helpers
  - deterministic invalid-request handler branch
- `src/api/rest.rs`
  - `app_router` route wiring range including Lightning paths

## CI behavior

The Rust workflow runs:

```bash
python3 scripts/check_lightning_coverage.py --min-percent 90
```

The script:

1. Runs `cargo llvm-cov --lib --json`.
2. Computes covered/instrumented lines for the scoped ranges above.
3. Fails CI if aggregate scoped line coverage drops below `90%`.

If source moves these logic blocks, update the line ranges in `scripts/check_lightning_coverage.py`.
