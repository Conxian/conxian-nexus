# Conxian Nexus (Glass Node)

Conxian Nexus is a high-performance Rust-based middleware designed to synchronize off-chain state with Stacks L1 truth. It acts as a "Glass Node," providing cryptographic proofs of on-chain data and enforcing transaction ordering to mitigate MEV.

## Core Features

- **Nakamoto Awareness**: Tracks Stacks Epoch 3.0/3.1 finality, differentiating between microblock soft-finality and burn-block hard-finality.
- **FSOC Sequencer**: Implements "First-Seen-On-Chain" (FSOC) transaction ordering to prevent front-running.
- **Sovereign Handoff**: A safety protocol that monitors sync drift and enables "Direct Withdrawal Tenure" if the Nexus falls behind.
- **Cryptographic Verification**: Provides a `/v1/proof` endpoint for verifiable state matching against the Stacks MARF tip.

## Architecture

The Nexus is composed of several functional modules:
- **nexus-sync**: Ingests Stacks node events via WebSocket and updates local persistence.
- **nexus-executor**: specialized execution environment for high-frequency internal trades and rebalancing.
- **nexus-safety**: Heartbeat service for health monitoring and safety mode triggers.
- **API (REST & gRPC)**: High-throughput interfaces for external and internal communication.

## Directory Structure

```
.
├── src/
│   ├── api/          # REST and gRPC implementations
│   ├── executor/     # FSOC sequencer and trade logic
│   ├── safety/       # Drift monitoring and safety mode
│   ├── storage/      # Database and Redis connections
│   ├── sync/         # Stacks L1 event ingestion
│   └── main.rs       # Application entry point
├── proto/            # Protobuf definitions
├── docs/             # OpenAPI specifications
└── migrations/      # SQLx database migrations
```

## Tech Stack

- **Runtime**: Rust (Tokio/Axum)
- **Database**: PostgreSQL (State persistence via SQLx)
- **Cache**: Redis (Fast state caching)
- **Communication**: gRPC (Tonic/Prost) and REST (Axum)

## Getting Started

### Prerequisites

- Rust (latest stable)
- PostgreSQL
- Redis
- *Note: Protobuf compiler is bundled via `protoc-bin-vendored`*

### Installation

```bash
git clone https://github.com/Conxian/conxian-nexus
cd conxian-nexus
cargo build
```

### Running the Service

```bash
cp .env.example .env
# Update .env with your DATABASE_URL and REDIS_URL
cargo run
```

## API Documentation

- **REST API**: See [OpenAPI Spec](docs/openapi.yaml) (Running on port 3000)
- **gRPC**: See [Protobuf Definition](proto/nexus.proto) (Running on port 50051)

## Testing

```bash
cargo test
```

## License

This project is licensed under the BSL 1.1 License - see the [LICENSE](LICENSE) file for details.
