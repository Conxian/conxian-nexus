# Conxian Nexus

Conxian Nexus is a middleware and proof layer that synchronizes off-chain state with Stacks L1 and exposes verifiable state services.

## Purpose

Provide a verifiable synchronization, ordering, and proof layer for Conxian services that need authoritative off-chain state aligned with chain activity.

## Status

**Active development (v0.4.7).** Production intent exists, but operators should validate readiness and deployment assumptions before use in critical environments.

## Scope

This repository focuses on synchronization, proof generation, execution ordering, and service interfaces. It does not represent company administrative systems or private operational workflows.

## Governance relation

This repository is maintained by Conxian Labs as public infrastructure supporting the Conxian ecosystem. It may underpin protocol-adjacent services while governance of the broader protocol evolves toward greater decentralization after mainnet.

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

## Policies

- [CONTRIBUTING.md](./CONTRIBUTING.md)
- [SECURITY.md](./SECURITY.md)
- [CHANGELOG.md](./CHANGELOG.md)
- [docs/CONTROL_MODEL.md](./docs/CONTROL_MODEL.md)

## Contact

- Support: [support@conxian-labs.com](mailto:support@conxian-labs.com)
- Security: [security@conxian-labs.com](mailto:security@conxian-labs.com)
- General: [info@conxian-labs.com](mailto:info@conxian-labs.com)

## License

BSL 1.1
