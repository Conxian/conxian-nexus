# Contributing to Conxian Nexus

Thank you for your interest in contributing!

## Development Process

1. Fork the repository.
2. Create a new branch for your feature or bugfix.
3. Ensure all tests pass.
4. Submit a Pull Request with a detailed description of changes.

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
