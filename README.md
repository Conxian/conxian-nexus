# Conxian Nexus (Glass Node)

Conxian Nexus is a protocol-first "Glass Node" and proof layer that synchronizes off-chain state with Stacks L1 and exposes verifiable state services for Tier 1 Chain Families (Bitcoin, EVM, Cosmos).

## Purpose

Provide a verifiable synchronization, ordering, and proof layer for Conxian services that need authoritative off-chain state aligned with chain activity. It serves as the primary observation and verification point for multi-chain state transitions.

## Status

**Active development (v0.4.13).** Production intent exists. Nexus is currently being hardened for Tier 1 multi-chain monitoring as per ADR-006. Operators should validate readiness and deployment assumptions before use in critical environments.

## Scope

This repository focuses on synchronization, proof generation, execution ordering, and service interfaces. It does not represent company administrative systems, legal workflows, or private operational records.

## Governance relation

This repository is maintained by Conxian Labs as public infrastructure supporting the Conxian ecosystem. It may underpin protocol-adjacent services while governance of the broader protocol evolves toward greater decentralization after mainnet.

## Relationship to the Conxian stack

- `Conxian` is the protocol core.
- `conxian-gateway` is the integration and middleware surface.
- `conxian_ui` and `conxius-wallet` are public application clients that may consume Nexus-backed services.
- `lib-conxian-core` provides shared primitives used across the stack.

## Modules

- `nexus-sync`: chain ingestion and reorg handling
- `nexus-state`: cryptographic state root and persistence
- `nexus-executor`: execution environment and sequencing logic
- `nexus-safety`: drift monitoring and safety mode
- `api`: REST and gRPC surfaces
- `oracle`: aggregated external data inputs where configured

## Getting started

### Prerequisites

- Docker and Docker Compose
- or Rust 1.82+, PostgreSQL 15, and Redis 7

### Running

```bash
docker-compose up --build
```

Or:

```bash
cargo run
```

### Testing

```bash
cargo test
```

## Release discipline

- Follow Semantic Versioning.
- Create annotated tags as `vX.Y.Z`.
- Document each release in [CHANGELOG.md](./CHANGELOG.md).
- Use [RELEASE.md](./docs/RELEASE.md) for the release workflow.

## Security

Do not disclose vulnerabilities publicly. Use [SECURITY.md](./SECURITY.md) or `security@conxian-labs.com`.

## Policies

- [CONTRIBUTING.md](./CONTRIBUTING.md)
- [SECURITY.md](./SECURITY.md)
- [CHANGELOG.md](./CHANGELOG.md)
- [CODEOWNERS](./.github/CODEOWNERS)
- [REPO_OWNERSHIP.md](./docs/REPO_OWNERSHIP.md)
- [RELEASE.md](./docs/RELEASE.md)
- [LICENSE](./LICENSE)

## Contact

- General: [info@conxian-labs.com](mailto:info@conxian-labs.com)
- Support: [support@conxian-labs.com](mailto:support@conxian-labs.com)
- Security: [security@conxian-labs.com](mailto:security@conxian-labs.com)

## License

BSL 1.1
