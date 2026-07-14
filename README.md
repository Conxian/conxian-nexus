# Conxian Nexus (Glass Node)

Conxian Nexus is a protocol-first "Glass Node" and proof layer, serving as the primary observation, synchronization, and verification point for Tier 1 Chain Families in the Conxian ecosystem.

## Purpose

Provide a verifiable synchronization, ordering, and proof layer for Conxian services. Nexus ensures authoritative off-chain state remains aligned with multi-chain activity (Bitcoin, EVM, Cosmos) via cryptographic state root commitments (MMR).

## Status

**Active development (v0.4.19).** Production intent exists. Nexus is currently being hardened for Tier 1 multi-chain monitoring as per ADR-006 and implementing the SRL-1 Lightning Resilience layer.

## Audience

- **Infrastructure Operators:** Deploying high-availability synchronization and verification nodes.
- **Protocol Developers:** Building applications that require deterministic, cross-chain event feeds and state proofs.
- **Security Auditors:** Verifying the integrity of off-chain state commitments against L1 finality.

## Scope

This repository owns the "Glass Node" implementation, multi-chain state normalization, and verifiable service interfaces. It is a core protocol component.

## Governance relation

Maintained by Conxian-Labs as public infrastructure. It provides the proof baseline for the Conxian Gateway and public application clients, maintaining strict boundary rules between observation and execution.

## Relationship to the Conxian stack

- **Core Protocol:** `Conxian` (DAO/On-chain) <-> **Nexus** (Observation/Proof)
- **Middleware:** `conxian-gateway` (Transport/RPC Adaptation)
- **Libraries:** `lib-conxian-core` (Shared Primitives)
- **Clients:** `conxius-wallet`, `conxian_ui`

## Architecture

Nexus is designed as a modular "Glass Node" that provides a verifiable synchronization layer between Layer 1 blockchains and the Conxian ecosystem.

- **Verifiable Proofs**: Generates MMR state root commitments for off-chain state.
- **Multi-Chain Adapters**: Standardized normalization for UTXO, EVM, and Cosmos families (see [ADR-006](./docs/ADR-006_Tier1_Chain_Families.md)).
- **Resilience**: Integrated SRL-1 recovery layer for Lightning Network reliability.

## Modules

- `nexus-sync`: multi-chain ingestion and reorg handling (Bitcoin, EVM, Cosmos).
- `nexus-state`: MMR state root commitments and persistence.
- `nexus-executor`: Protocol adapters (BitVM2, RGB, Stacks) and sequencing logic.
- `nexus-safety`: Drift monitoring and SRL-1 resilience layer.
- `api`: REST and gRPC surfaces for proofs and event feeds.

## Documentation

Comprehensive documentation is available at [docs.conxian-labs.com/nexus](https://docs.conxian-labs.com/nexus) (GitHub Pages route).

- [Operator Guide](./docs/PRD.md)
- [API Reference](./docs/openapi.yaml)
- [Security Model](./SECURITY.md)
- [Observability & Runbooks](./docs/remediation/OBSERVABILITY_RUNBOOK.md)

## Getting started

### Prerequisites

- Docker and Docker Compose
- Rust 1.82+, PostgreSQL 15, and Redis 7

### Setup

1.  **Environment Configuration**:
    Copy the example environment file and configure your secrets:
    ```bash
    cp .env.example .env
    ```
    *Note: Ensure `DATABASE_URL` and `REDIS_URL` are correctly set for your local or docker environment.*

2.  **Database Migrations**:
    Nexus requires a PostgreSQL database. Apply migrations using `sqlx`:
    ```bash
    cargo install sqlx-cli --no-default-features --features postgres
    sqlx migrate run
    ```

### Quick Start (Docker)

If you prefer using Docker, you can start the entire stack (including Postgres and Redis) with:

```bash
docker-compose up --build
```

For more detailed setup instructions, including production hardening, see the [Operator Guide](./docs/PRD.md).

## Policies

- [CONTRIBUTING.md](./CONTRIBUTING.md)
- [SECURITY.md](./SECURITY.md)
- [SUPPORT.md](./SUPPORT.md)
- [CHANGELOG.md](./CHANGELOG.md)
- [CODEOWNERS](./.github/CODEOWNERS)
- [REPO_OWNERSHIP.md](./docs/REPO_OWNERSHIP.md)
- [LICENSE](./LICENSE)

## Contact

- Support: [support@conxian-labs.com](mailto:support@conxian-labs.com) (See [SUPPORT.md](./SUPPORT.md) for details)
- Security: [security@conxian-labs.com](mailto:security@conxian-labs.com) (See [SECURITY.md](./SECURITY.md) for details)

## License

BSL 1.1
