# Administrative workflow persistence

Claim state: implemented persistence and API candidate; production intent pending
normal review, release, migration and operation-specific acceptance. Coordination:
Business #1390 / #1423 / #1349 and the existing Nexus owner boundary.

Administrative release/governance projections are operational records. They are
not protocol settlement truth, signing authority or deployment acceptance. Nexus
still observes and verifies; execution and custody remain separate boundaries.

## Storage and commit behavior

The `20261024000000_admin_workflows.sql` migration creates bounded entity records
and an associated command/event/receipt table in the existing PostgreSQL database.
Use the normal reviewed migration path; no additional cloud database is required.
The schema and store work on operator-hosted PostgreSQL as well as managed
PostgreSQL. This portability does not itself establish independent operation.

Each store command validates its input, locks its command/entity identities,
checks the current lifecycle, updates the projection, inserts the audit event
and commits one transaction with synchronous commit enabled before returning a
success receipt. A failed audit insert rolls back the projection too. A retry
with the same key and semantic input returns the stored receipt; changed input
under the same key returns conflict. Concurrent competing decisions cannot both
change the same pending entity. Store errors are redacted at the HTTP boundary.

This is conditional database durability under the selected PostgreSQL storage
configuration, not a claim of replica quorum, tamper-proof history, crash recovery,
disaster recovery or independent governance acceptance. The pool-recreation test
checks process-state independence; a separate cold backup/restore drill remains
necessary before migration acceptance.

## HTTP boundary

Existing authentication and signature-policy checks remain in place. The actor
label remains the existing authorised request's attribution, not a new identity
attestation. This implementation does not upgrade the qualification of that
authentication/signing policy, introduce a signer, or broadcast anything.

- `POST /admin/v1/releases/request-approval` commits an `in_review` projection and
  returns `requestId` plus the committed `auditEventId`.
- `POST /admin/v1/releases/decision` returns `decisionId` plus `auditEventId`.
- `POST /admin/v1/governance/decision` accepts canonical `actorId` and the legacy
  `actor_id` spelling, and returns `decisionId` plus `auditEventId`.
- A decision requires a registered pending entity; it does not silently create
  an approved release/action. Release decisions allow only `in_review` as the
  source. Governance decisions allow only `pending` as the source.
- Release request-change decisions return to `draft`; governance request-change
  decisions produce `changes_requested`. Terminal states reject a new conflicting
  decision. A stored retry remains a historical receipt, not current-state proof.

New callers may send `Idempotency-Key` (a nonempty bounded identifier, never a
credential) and retain it across retries. Older callers use a deterministic key
over the semantic command. The latter cannot distinguish a later intentional
identical resubmission from a retry; use an explicit new key after a valid lifecycle
re-entry. Idempotency keys are not cryptographic nonces or replay authorisation.

Governance intake is a trusted owner-side `ProposeGovernance` store operation.
No unauthenticated/public registration route is introduced. The owner must qualify
its canonical proposal ingestion before expecting real actions in the collection.

`GET /admin/v1/releases`, `GET /admin/v1/governance-actions` and
`GET /admin/v1/audit-events` read actual persisted rows after authentication.
They accept `limit` (1–1000, default 100) and `offset` (0–1000000). Responses
include `limit`, `offset` and a conservative `nextOffset` whenever the page is full;
the final successor may be empty. Offset pages are not a stable snapshot during
concurrent writes. Consumers doing complete evidence collection must follow pages
and separately establish collection consistency. Notes stay in the event store
and are excluded from the collection response.

Unknown entities return 404, changed-key bindings or invalid transitions return
409, invalid inputs return 400, and unavailable/missing/malformed store state
returns 503. HTTP work is bounded by a five-second timeout; a caller that loses
the commit response must retry its original key and reconcile the returned receipt.
No synthetic success or empty collection is used as a database fallback.

## Verification and adoption

The conformance suite uses the existing explicit disposable fixture controls:
numeric loopback, a dedicated non-superuser `conformance` role, an explicit port,
the `conformance_test` database and a fresh per-case schema. It never reads generic
`DATABASE_URL`, truncates production tables or substitutes an in-memory store.

```sh
cargo test --locked --lib storage::admin_workflows
cargo test --locked --test admin_api_test
# Only with the approved disposable fixture configured:
cargo test --locked --test admin_workflow_conformance
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Cases cover real PostgreSQL commits and receipt read-back after pool recreation,
concurrent duplicate commands, conflicting keys/decisions, audit-insert rollback
and retry, missing entities/schema, lifecycle transitions, bounded pages and the
authenticated HTTP request/decision/audit journey. Signature strings in the debug
test fixture are test data, not hardware, cryptographic-quorum or live-user proof.
The separate auth/shape test router uses an intentionally unavailable port-1 database
and a short acquisition timeout; it never targets a developer application database.

The local runtime cannot start the PostgreSQL fixture because its user-switching
operations are restricted. Local compilation/unit/auth-negative checks and the
hosted PostgreSQL conformance result must therefore be reported separately.
The existing Rust CI job already runs the complete workspace against PostgreSQL 17
with that guarded fixture, so it runs the new real-database cases automatically.

Before adoption: require exact-source checks and independent review, apply the
migration in staging, verify actual auth policy and proposal intake, reconcile
read/write/event receipts, and exercise restart plus cold backup/restore. Preserve
the new tables/events during application rollback; dropping them is a separate
data-retention decision. This feature does not claim that all runtime readiness,
environment, safety or promotion-evidence contracts are now qualified.

Implementation follows PostgreSQL's documented
[transaction](https://www.postgresql.org/docs/current/tutorial-transactions.html)
and [transaction-scoped locking](https://www.postgresql.org/docs/current/explicit-locking.html)
semantics, checked on 24 October 2026.
