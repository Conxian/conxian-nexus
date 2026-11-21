# Neon PostgreSQL 19 Migration Runbook

Production database is Neon (managed). This runbook covers moving a Conxian
database from PostgreSQL 17 to 19 on Neon.

## What Neon does and does not do

Verified against the Neon changelog and upgrade docs (2026-11-20):

- **PostgreSQL 19 is supported from 2026-11-19** on newly created projects.
- **Neon does not auto-upgrade the major version of an existing project.**
- A project is **locked to the major version chosen at creation**. Minor
  version upgrades within a major are applied automatically; major upgrades
  are not.
- **`pg_upgrade` is not available on Neon.** It cannot be run against a Neon
  project.

Migration therefore means **create a new PostgreSQL 19 project and move the
data into it**. There is no in-place path.

## Prerequisites

- Neon account access with permission to create projects and branches.
- A `pg_dump` / `pg_restore` client at version **17 or newer** (must be at
  least the source major version).
- The application stopped, or a write freeze in place, for the final cutover.

## Steps

### 1. Create the PostgreSQL 19 project

In the Neon console, create a new project and **select PostgreSQL 19** at
creation time. This choice cannot be changed later.

Record the new project's connection string.

### 2. Apply the schema to the new project

Apply migrations to the empty PostgreSQL 19 database before loading data. This
uses the same migrations CI uses and keeps the new project consistent with the
code that will connect to it.

For nexus, all 25 migrations in `migrations/` apply cleanly to PostgreSQL 19
(verified locally: 0 failures, 29 tables).

### 3. Copy the data

Two supported paths:

**Option A - `pg_dump` / `pg_restore` (recommended for a maintenance window)**

```bash
pg_dump --no-owner --no-privileges --format=custom \
  --file=conxian.dump "$SOURCE_DATABASE_URL"

pg_restore --no-owner --no-privileges --clean --if-exists \
  --dbname="$TARGET_DATABASE_URL" conxian.dump
```

**Option B - logical replication (near-zero downtime)**

Set up logical replication from the PostgreSQL 17 project to the PostgreSQL 19
project, let it catch up, then promote. Use this when the downtime window in
Option A is too long.

The Neon console also offers an **Import Data Assistant** that drives the same
copy for you.

### 4. Verify before cutover

Run these against the new project and compare with the source:

```sql
SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public';
SELECT version();
```

Spot-check row counts on the largest tables, and run the application's own
conformance suite against the new connection string.

### 5. Cut over

1. Stop writers (or enter the write freeze).
2. Do a final delta copy if you used Option A.
3. Point `DATABASE_URL` at the PostgreSQL 19 project.
4. Restart the application and confirm health checks pass.
5. Keep the old project intact until the rollback window closes.

### 6. Roll back

Repoint `DATABASE_URL` at the old PostgreSQL 17 project. This is why the old
project is not deleted until the window closes.

## Notes for this repository

- CI conformance fixtures now run PostgreSQL 19, matching production after the
  move. See `.github/workflows/rust.yml` and `.github/workflows/release.yml`.
- The local `docker-compose.yml` database is PostgreSQL 19. Its service has no
  volume, so the PostgreSQL 18+ `PGDATA` relocation does not affect it.
- No migration uses `CREATE TYPE` or `ALTER TYPE`, so the enum-related changes
  in PostgreSQL 18/19 are not triggered.
