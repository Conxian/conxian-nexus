# Conxian Nexus (Glass Node)

Conxian Nexus is a high-performance middleware designed to synchronize off-chain state with Stacks L1, providing cryptographic proofs and enforcing transaction ordering.

## Purpose

Provide a verifiable synchronization and proof layer between off-chain state and Stacks L1, with ordering guarantees and safety-mode controls.

## Status

Active development. Module boundaries and APIs may change as the stack converges on shared primitives.

## Ownership

Ownership and review requirements are defined in [`CODEOWNERS`](./CODEOWNERS).

## Audience

- Backend engineers working on state sync, proof generation, and ordering.
- Protocol engineers integrating L1 events into higher-level execution flows.
- Infrastructure operators running Nexus for observability and safety monitoring.

## Relationship to the Conxian stack

- Complements Conxian Gateway by focusing on ordering, proofs, and state-root integrity.
- Can be used as an execution-adjacent component for systems that require FSOC-style validation.

## Modules

- **nexus-sync**: Ingests Stacks L1 events via RPC polling, updates local state, and handles microblock reorgs with automated rollback.
- **nexus-state**: Manages the cryptographic state root using a Merkle tree of transaction IDs and a persistent Merkle Mountain Range (MMR).
- **nexus-executor**: specialized execution environment with FSOC (First-Seen-On-Chain) sequencer logic.
- **nexus-safety**: Monitors sync drift and triggers Safety Mode (Sovereign Handoff).
- **API (REST & gRPC)**: Interfaces for state verification and transaction execution.
- **lib-conxian-core**: Shared library for multi-protocol support (Bisq, RGB, BitVM).
- **oracle**: Multi-source aggregated FX rate provider with on-chain state pushing.

## Features

- **Nakamoto Ready**: Handles microblocks and burn blocks.
- **FSOC Sequencer**: Mitigates MEV by validating transaction timestamps and payloads against on-chain events.
- **Sovereign Handoff**: Automatic safety mode if sync drift exceeds threshold.
- **Verifiable Proofs**: Generate and verify Merkle proofs for any transaction.
- **Persistent MMR**: Full persistence of MMR peaks and nodes in PostgreSQL, with support for cryptographic inclusion proofs.
- **Multi-Protocol**: Unified support for Bisq, RGB, and BitVM.
- **Observability**: Prometheus metrics exporter and internal JSON metrics.

## API Highlights

- `GET /v1/status`: System status and state root.
- `GET /v1/metrics`: System performance metrics (JSON).
- `GET /metrics`: Prometheus metrics exporter (Text).
- `POST /v1/execute`: Submit transactions for FSOC validation.
- `GET /v1/proof?key=<tx_id>`: Retrieve Merkle proof.
- `GET /v1/mmr-proof?tx_id=<tx_id>`: Retrieve MMR inclusion proof.
- `GET /v1/services`: Multi-protocol service health.

## Getting Started

### Prerequisites

- Docker and Docker Compose
- *Or* Rust 1.82+, PostgreSQL 15, and Redis 7

### Running

```bash
docker-compose up --build
```

Or manually:

```bash
cargo run
```

## Documentation

- **PRD**: [docs/PRD.md](docs/PRD.md)
- **API Spec**: [docs/openapi.yaml](docs/openapi.yaml)
- **Changelog**: [CHANGELOG.md](CHANGELOG.md)

## License

BSL 1.1
