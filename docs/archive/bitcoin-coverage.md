# Bitcoin Coverage Gate

Nexus enforces Bitcoin-focused coverage with `scripts/check_bitcoin_coverage.py`.

## Why this is line-range scoped (not full-file)

`cargo llvm-cov` built-in fail-under thresholds are file/workspace-wide. Our Bitcoin logic is concentrated in mixed files (`src/api/rest.rs` includes many unrelated routes), so file-level thresholds would fail on unrelated low-coverage regions and create noisy regressions.

To keep the gate actionable, we enforce line coverage on Bitcoin-specific ranges:

- `src/api/dlc.rs`
  - DLC request validation and announcement/signing helpers
  - deterministic invalid-request handler branch
- `src/api/rest.rs`
  - RGB contract route wiring in `app_router`
  - RGB contract endpoint handler and error mapping
- `src/executor/rgb.rs`
  - rollout-mode behavior and display
  - lookup validation and mode-specific branches (`disabled`, `shadow`, `active`)

## CI policy

The Rust workflow runs:

```bash
python3 scripts/check_bitcoin_coverage.py --min-percent 95
```

The script:

1. Runs `cargo llvm-cov --lib --json`.
2. Computes covered/instrumented lines for the Bitcoin-scoped ranges above.
3. Fails CI if aggregate scoped line coverage drops below `95%`.

If source moves these logic blocks, update line ranges in `scripts/check_bitcoin_coverage.py`.
