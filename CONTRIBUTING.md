# Contributing to Conxian Nexus

Thank you for your interest in contributing!

## Development Process

1. Fork the repository.
2. Create a new branch for your feature or bugfix.
3. Ensure all tests pass.
4. Submit a Pull Request with a detailed description of changes.

## Local Development

To set up your local environment for development:

1. **Environment**: Copy `.env.example` to `.env` and configure your local PostgreSQL and Redis connection strings.
2. **Database**: Run migrations using `sqlx-cli` or ensure your local Postgres matches the schema in `migrations/`.
3. **Tests**: Run `cargo test` to verify your changes. Some integration tests may skip if local infrastructure (Postgres/Redis) is unavailable.
4. **Boundary Check**: Run `./scripts/check_production_boundary.sh` to ensure no testnet principals are introduced.

## Coding Standards

- Follow standard Rust formatting (`cargo fmt`).
- Ensure all public functions have doc comments.
- Maintain high test coverage for new logic.
- **Do not commit source code dumps, audit logs, or temporary artifacts**. Check `.gitignore` for current patterns.

## Contact

For operational questions (non-security), you can reach:

- Maintainer: [@botshelomokoka](https://github.com/botshelomokoka) (botshelo [at] conxian-labs.com)
- Shared inboxes: support [at] conxian-labs.com, info [at] conxian-labs.com, admin [at] conxian-labs.com

For security reports, follow [`SECURITY.md`](./SECURITY.md).

## Core Modules Update
- **NexusState**: Now uses a native Merkle Tree for verifiable state root tracking.
- **lib-conxian-core**: Enhanced with a full Wallet implementation (k256/ECDSA).
