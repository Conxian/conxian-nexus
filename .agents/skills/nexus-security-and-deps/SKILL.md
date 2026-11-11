---
name: nexus-security-and-deps
description: This skill should be used when changing nexus's storage init, safety mode, dependency/audit gates, or license boundary — the parts governed by ADR-007 (network-agnostic) and the RUSTSEC remediation.
---

# Nexus security, storage, and dependency gates

Nexus is the state/proof oracle. Its correctness and supply chain are governed by
org ADRs and fail-closed gates; changing them requires understanding the boundary.

## License boundary (ADR-009)

- `conxian-nexus` is **AGPL-3.0-or-later** (`Cargo.toml` `license`), not MIT.
  Because `internal/engine/src/billing.rs` is the single rate card (ADR-009), the
  monetization crates are AGPL so a fork cannot strip the rate card. Do not
  relicense nexus to a permissive license — it is a deliberate revenue moat and a
  funding-gate decision.

## Network-agnostic (ADR-007)

- There is no own chain, token, or `.clar` surface. Nexus observes/verifies across
  existing networks and settles on Bitcoin L1 via Stacks. Do not reintroduce
  own-protocol concepts (`revenue-automation.clar`, Fiscal-Vault-Oracle, etc.).

## Storage hardening

- `src/storage/mod.rs` requires **authenticated and remote** Redis and PostgreSQL
  in release builds. The emergency overrides are `NEXUS_ALLOW_UNSAFE_REDIS` and
  `NEXUS_ALLOW_UNSAFE_DB` — these are for infrastructure failures only, and
  setting them must be a deliberate, reviewed act (a localhost Redis with the
  override set is the documented emergency path, not the normal state).

## Safety mode

- `src/executor/mod.rs` gates value-bearing operations behind
  `check_safety_mode()`; `crate::safety::is_safety_mode_active(&storage)` decides.
  When safety mode is active the service is **up but paused** — report it as
  degraded, never as healthy. The gateway's status probe reads `safety_mode` from
  the health payload and classifies it `degraded`.

## Dependency / audit gates

- `.github/workflows/cargo-audit.yml` runs `cargo audit` with an explicit
  `--ignore RUSTSEC-...` list. **Do not silently add an ignore.** A finding is
  remediated by upgrading to a patched release; an ignore needs a documented,
  reviewable reason and an expiry condition.
- RUSTSEC-2026-0459 (`libsqlite3-sys`): published `sqlx 0.9.0` caps
  `libsqlite3-sys <0.38.0`. The fix pins `sqlx` to the canonical successor
  (`launchbadge/sqlx` redirects to `transact-rs/sqlx`) at an exact `rev` that
  widens the range, resolving `0.39.1`. `scripts/check_dependency_declarations.py`
  permits exactly that one pinned declaration and asserts the patched version.

## Tests to keep green

`cargo test --workspace`, `cargo clippy -D warnings`, `cargo fmt --check`,
`cargo audit`, and `scripts/check_dependency_declarations.py`.
