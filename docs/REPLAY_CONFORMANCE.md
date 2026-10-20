# Disposable PostgreSQL replay conformance

Owner: [#439](https://github.com/Conxian/conxian-nexus/issues/439), supporting the shared P0 replay gate in SDK #240. Nexus observes and proves; these tests confer no signing or execution authority.

`tests/idempotency_conformance.rs` requires both `CONFORMANCE_DATABASE_URL` and `ALLOW_DESTRUCTIVE_TEST_RESET=true`. Missing configuration fails immediately; `DATABASE_URL` is never read as a fallback. Use an explicit numeric loopback host, explicit port, nonempty password, database `conformance_test` and non-superuser role `conformance`. Query/fragment overrides and inherited `PG*` settings are rejected before connection. Validation errors do not echo URLs or credentials. The connected database, role and non-superuser status are checked on every new connection.

Every case creates a random `nexus_conformance_<uuid>` schema and installs the three actual replay migrations there in a transaction. All test pools use only that schema in their search path. There is no shared/public-table `TRUNCATE` or reset. A separate isolation case proves one test schema cannot reset another's records. Failed setup or unavailable PostgreSQL is a failed test, not a skip. Test schemas live only as long as the explicitly disposable database; destroy the local container when finished. Do not reuse an application database or give the conformance role production credentials.

## Local fixture

Run an isolated container, bound only to loopback:

```bash
docker run --rm --name nexus-conformance -e POSTGRES_DB=conformance_test -e POSTGRES_PASSWORD=postgres-fixture-only -p 127.0.0.1:5433:5432 postgres:17
```

After PostgreSQL is ready, in another terminal:

```bash
docker exec nexus-conformance psql -U postgres -d conformance_test -v ON_ERROR_STOP=1 -c "CREATE ROLE conformance LOGIN PASSWORD 'conformance-fixture-only'; ALTER DATABASE conformance_test OWNER TO conformance;"
CONFORMANCE_DATABASE_URL='postgres://conformance:conformance-fixture-only@127.0.0.1:5433/conformance_test' ALLOW_DESTRUCTIVE_TEST_RESET=true cargo test --workspace --locked
docker stop nexus-conformance
```

These are disposable fixture credentials, not production secrets. Unset inherited `PG*` variables before invoking Cargo; they can influence driver connection defaults. Pure configuration rejection tests need no database: `cargo test --locked --test conformance_config`. Running the full suite without the explicit fixture is expected to fail.

## CI and runner choice

Both existing Rust and Release Build & Test jobs create a job-local PostgreSQL 17 service with a dynamic mapped port and the dedicated non-superuser role. They run pure guard tests, prove missing configuration fails even when generic DATABASE_URL is set, prove an unavailable explicit fixture fails, then run the complete workspace suite against the service. No provider credentials or connected databases are used. GitHub tears down the service with the job.

Native GitHub Actions services were selected because they fit the existing CI with no new Rust dependency. Testcontainers-rs, pg_tmp and pg-embed are alternative local runners requiring their own installation/runtime qualification; none is installed or claimed supported by this change. A hosted database branch would require separately approved disposable/schema-only data, role and lifecycle controls; no Neon branch or production snapshot is used here. A local port or `_test` suffix alone is not sufficient isolation.

The suite covers consume-once, batch rollback/retry, 32-worker contention, retention, a standalone high-water clock, reconnect persistence, lock lifecycle and schema isolation. Reconnect is not a server crash/failover test. The consume/time/expiry APIs are not yet independently qualified as the SDK's atomic production replay contract. Actual Neon application binding, replica/region, outage/recovery, provider evidence and independent release acceptance remain open under #439 and SDK #240/#202. Historical #251 source implementation is not current production acceptance.

Primary references: [GitHub PostgreSQL services](https://docs.github.com/en/actions/tutorials/use-containerized-services/create-postgresql-service-containers) and [SQLx connection options](https://docs.rs/sqlx/latest/sqlx/postgres/struct.PgConnectOptions.html). The pinned SQLx 0.9 source was also reviewed for inherited connection defaults; explicit test URLs must not be overridden by them.
