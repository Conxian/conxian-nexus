# Conxian Nexus (Glass Node)

Conxian Nexus is a high-performance middleware designed to synchronize off-chain state with Stacks L1, providing cryptographic proofs and enforcing transaction ordering.

## Modules

- **nexus-sync**: Ingests Stacks L1 events via RPC polling and updates local state.
- **nexus-state**: Manages the cryptographic state root using a Merkle tree of transaction IDs.
- **nexus-executor**: specialized execution environment with FSOC (First-Seen-On-Chain) sequencer logic.
- **nexus-safety**: Monitors sync drift and triggers Safety Mode (Sovereign Handoff).
- **API (REST & gRPC)**: Interfaces for state verification and transaction execution.
- **lib-conxian-core**: Shared library for multi-protocol support (Bisq, RGB, BitVM).

## Features

- **Nakamoto Ready**: Handles microblocks and burn blocks.
- **FSOC Sequencer**: Mitigates MEV by validating transaction timestamps against on-chain events.
- **Sovereign Handoff**: Automatic safety mode if sync drift exceeds threshold.
- **Verifiable Proofs**: Generate and verify Merkle proofs for any transaction.
- **Multi-Protocol**: Unified support for Bisq, RGB, and BitVM.
- **Observability**: Prometheus metrics exporter and internal JSON metrics.

## API Highlights

- `GET /v1/status`: System status and state root.
- `GET /v1/metrics`: System performance metrics (JSON).
- `GET /metrics`: Prometheus metrics exporter (Text).
- `POST /v1/execute`: Submit transactions for FSOC validation.
- `GET /v1/proof?key=<tx_id>`: Retrieve Merkle proof.
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
