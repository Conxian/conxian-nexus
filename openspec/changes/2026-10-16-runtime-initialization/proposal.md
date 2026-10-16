# Bounded runtime initialization

Refs: Conxian/conxian-business#1390, Conxian/conxian-business#1423, #418.

Honor the hosting platform's `PORT` when `REST_PORT` is absent, reject invalid
explicit ports, and build the release binary from the committed lockfile on the
same Debian release as the runtime. Keep `/live` independent of dependencies;
bound `/health` and the proof manifest's health observation and require both
PostgreSQL reachability and the existing Redis safety observation.

Validate configuration precedence and failures, bounded health behavior, the full
Rust suite and strict linting. Exercise the actual image with isolated authenticated
PostgreSQL/Redis, migrations, dependency outage/recovery and restart in CI.
These checks establish startup behavior, not chain synchronization, payment,
provider qualification, release acceptance or production authorization.
