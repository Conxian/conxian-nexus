# Runtime initialization

`REST_PORT` is the explicit HTTP-port override. When it is absent, `PORT` is used;
when both are absent, the default is 3000. Invalid, empty, zero and out-of-range
explicit values fail configuration. gRPC continues to use `GRPC_PORT` separately.

`GET /live` reports process liveness. `GET /health`, `/v1/status` and the proof
manifest's health field require a PostgreSQL query and the existing Redis safety
observation within a two-second total deadline. Missing observations return 503;
an observed active safety mode also returns 503. Raw backend errors are omitted.
These checks do not establish chain freshness, proof/provider qualification,
paid entitlements or independent release acceptance.

The container uses Rust 1.98.1 and Debian bookworm in both build and runtime stages,
and builds the committed dependency graph using `cargo build --release --locked`.
No database or Redis development-default override is enabled by the image.

With Docker available, run:

```sh
docker build --tag conxian-nexus:init .
python3 scripts/container_init_smoke.py --image conxian-nexus:init
```

The disposable initialization check uses isolated authenticated PostgreSQL/Redis,
runs actual migrations and tests readiness, denied administration, safety mode,
dependency timeout/recovery and restart. It creates no production data or release.
The matching read-only CI workflow runs on relevant PRs and manual dispatch.

Deployment still requires the actual authorized database/cache, current runtime
configuration, qualified operation-specific dependencies, source/artifact evidence,
reviewed promotion and rollback/acceptance evidence under the existing owner tracks.
