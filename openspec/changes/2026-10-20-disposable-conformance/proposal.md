# Isolate and require PostgreSQL replay conformance

Owner: Nexus #439; shared P0 trust/replay gate SDK #240; portfolio Business #1390.

Require CONFORMANCE_DATABASE_URL and ALLOW_DESTRUCTIVE_TEST_RESET=true. Missing/invalid configuration fails before connection; DATABASE_URL is never a fallback. Require an explicit numeric loopback host, port, nonempty password, exact conformance_test database and conformance role. Reject URL query/fragment overrides and inherited PostgreSQL connection settings. Errors never print connection strings or credentials.

Replace shared-table TRUNCATE with a randomly named private schema per test. Create the three replay tables from their actual migration files inside that schema in a transaction, then constrain every test connection's search_path to that schema. Database/user read-back must match the selected fixture. A failure to create/install/connect is a test failure. No cleanup query targets application/public tables; job/container teardown disposes of test schemas even on failure.

Provision a job-local PostgreSQL service in both existing Rust and Release Build & Test jobs. Use a non-superuser conformance role, unique mapped port, synthetic credentials and no production/provider secrets. Keep workspace tests mandatory, add pure guard tests and negative missing-config execution, and retain real contention/rollback/reconnect tests. Document reconnect versus server crash/failover limits and correct historical source-only completion claims.

Selected runner: native GitHub Actions service container; no new Rust runner dependency. Testcontainers/pg_tmp/pg-embed can later provide the same guarded disposable fixture locally. A hosted Neon branch is not selected: branching may inherit production data and needs distinct approved schema-only provisioning and cleanup evidence. No production resources or credentials are needed here. Normal exact-head review and release/provider qualification remain separate.

Primary references: https://docs.github.com/en/actions/tutorials/use-containerized-services/create-postgresql-service-containers ; https://docs.rs/sqlx/latest/sqlx/postgres/struct.PgConnectOptions.html . Both the explicit URL and the effective driver options must remain inside the fixture boundary.
