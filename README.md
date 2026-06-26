# Conxian Nexus (Glass Node)

Conxian Nexus is a protocol-first "Glass Node" and proof layer, serving as the primary observation, synchronization, and verification point for Tier 1 Chain Families in the Conxian ecosystem.

## Purpose

Provide a verifiable synchronization, ordering, and proof layer for Conxian services. Nexus ensures authoritative off-chain state remains aligned with multi-chain activity (Bitcoin, EVM, Cosmos) via cryptographic state root commitments (MMR).

## Status

**Active development (v0.4.17).** Production intent exists. Nexus is currently being hardened for Tier 1 multi-chain monitoring as per ADR-006 and implementing the SRL-1 Lightning Resilience layer.

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

## Modules

- `nexus-sync`: multi-chain ingestion and reorg handling (Bitcoin, EVM, Cosmos).
- `nexus-state`: MMR state root commitments and persistence.
- `nexus-executor`: Protocol adapters (BitVM2, RGB, Stacks) and sequencing logic.
- `nexus-safety`: Drift monitoring and SRL-1 resilience layer.
- `api`: REST and gRPC surfaces for proofs and event feeds.

## Documentation

Comprehensive documentation is available at [docs.conxian-labs.com/nexus](https://docs.conxian-labs.com/nexus) (GitHub Pages route).

- [Architecture & ADRs](./docs/ADR-006_Tier1_Chain_Families.md)
- [Operator Guide](./docs/PRD.md)
- [API Reference](./docs/openapi.yaml)
- [Security Model](./SECURITY.md)
- [Observability & Runbooks](./docs/remediation/OBSERVABILITY_RUNBOOK.md)

## Getting started

### Prerequisites

- Docker and Docker Compose
- Rust 1.82+, PostgreSQL 15, and Redis 7

### Quick Start

```bash
docker-compose up --build
```

## Policies

- [CONTRIBUTING.md](./CONTRIBUTING.md)
- [SECURITY.md](./SECURITY.md)
- [CHANGELOG.md](./CHANGELOG.md)
- [CODEOWNERS](./.github/CODEOWNERS)
- [REPO_OWNERSHIP.md](./docs/REPO_OWNERSHIP.md)
- [LICENSE](./LICENSE)

## Contact

- Support: [support@conxian-labs.com](mailto:support@conxian-labs.com)
- Security: [security@conxian-labs.com](mailto:security@conxian-labs.com)

## License

BSL 1.1
