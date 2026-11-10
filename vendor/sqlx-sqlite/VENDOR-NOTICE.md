# Vendored `sqlx-sqlite` (RUSTSEC-2026-0459 mitigation)

This directory contains a vendored copy of `sqlx-sqlite 0.9.0` with **one**
change: the `libsqlite3-sys` requirement is widened from
`>=0.30.1, <0.38.0` to `>=0.30.1, <0.40.0` (see `Cargo.toml`).

## Why

`cargo audit` fails on **RUSTSEC-2026-0459** (`libsqlite3-sys` via CVE-2026-119375);
the patched version is `libsqlite3-sys >= 0.39.0`. `sqlx-sqlite 0.9.0` pins
`libsqlite3-sys < 0.38.0`, and a 0.x minor bump is semver-incompatible, so the
patched crate cannot be selected upstream. No newer `sqlx`/`sqlx-sqlite` release
exists yet that widens the requirement.

`[patch.crates-io]` in the root `Cargo.toml` redirects `sqlx-sqlite` here, so the
lock resolves `libsqlite3-sys 0.39.1`.

## Scope / reachability

Nexus uses **PostgreSQL only** — `src/` has zero `sqlite` references — so the
sqlite backend is not exercised at runtime; this is a lockfile-graph fix for the
audit gate.

## Verified

- `cargo audit` -> RUSTSEC-2026-0459 gone.
- `cargo check --workspace --locked` -> clean.
- `cargo test --workspace` -> unit tests pass (DB-gated conformance tests need
  `CONFORMANCE_DATABASE_URL`, unchanged).

## Removal

Delete this directory and the `[patch.crates-io]` block in the root `Cargo.toml`
as soon as a `sqlx`/`sqlx-sqlite` release ships with `libsqlite3-sys >= 0.39`
(see issue #565). Then `cargo update -p sqlx-sqlite`.

Source: crates.io `sqlx-sqlite 0.9.0` (MIT OR Apache-2.0; license files retained).
