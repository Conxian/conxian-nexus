# Conxian Nexus (Glass Node)

Conxian Nexus is a high-performance Rust-based middleware designed to synchronize off-chain state with Stacks L1 truth. It acts as a "Glass Node," providing cryptographic proofs of on-chain data and enforcing transaction ordering to mitigate MEV.

## Core Features

- **Unified Architecture**: Consumes `lib-conxian-core` to run Multi-Protocol Services (Bisq/RGB/BitVM) alongside core Network Health logic.
- **Glass Node Activated**: Real-world awareness via Stacks Node RPC polling for accurate burn-block height tracking.
- **Nakamoto Awareness**: Tracks Stacks Epoch 3.0/3.1 finality, differentiating between microblock soft-finality and burn-block hard-finality.
- **FSOC Sequencer**: Implements "First-Seen-On-Chain" (FSOC) transaction ordering to prevent front-running.
- **Sovereign Handoff**: A safety protocol that monitors sync drift and enables "Direct Withdrawal Tenure" if the Nexus falls behind.
- **Cryptographic Verification**: Provides a `/v1/proof` endpoint for verifiable state matching against the Stacks MARF tip.

## Architecture

The Nexus is composed of several functional modules:
- **nexus-sync**: Ingests Stacks node events (simulated or real) and updates local persistence.
- **nexus-executor**: specialized execution environment for high-frequency internal trades and rebalancing.
- **nexus-safety**: Heartbeat service for health monitoring and safety mode triggers based on real L1 height.
- **API (REST & gRPC)**: High-throughput interfaces for external and internal communication.
- **lib-conxian-core**: Shared library for wallet logic and multi-protocol gateway services.

## Getting Started

### Prerequisites

- Docker and Docker Compose
- *Or* Rust (latest stable), PostgreSQL, and Redis

### Running with Docker (Recommended)

```bash
docker-compose up --build
```

This will start the Nexus node, PostgreSQL 15, and Redis 7.

### Manual Installation

1. **Install Dependencies**:
   ```bash
   cargo build
   ```

2. **Run Migrations**:
   Ensure PostgreSQL is running and `DATABASE_URL` is set.
   ```bash
   sqlx migrate run
   ```

3. **Run the Service**:
   ```bash
   cp .env.example .env
   # Update .env with your DATABASE_URL, REDIS_URL, and STACKS_NODE_RPC_URL
   cargo run
   ```

## API Documentation

- **REST API**: Running on port 3000
  - `GET /v1/status`: System health and sync status.
  - `GET /v1/services`: Status of multi-protocol services (Bisq, RGB).
  - `GET /v1/proof?key=<tx_id>`: Merkle proof for a transaction.
  - `POST /v1/verify-state`: Verify a state root.
- **gRPC**: Running on port 50051 (See `proto/nexus.proto`)

## Testing

```bash
cargo test
```

## License

This project is licensed under the BSL 1.1 License - see the [LICENSE](LICENSE) file for details.
