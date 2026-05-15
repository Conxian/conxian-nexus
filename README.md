# Conxian Nexus

[![Version](https://img.shields.io/badge/version-0.4.7-blue.svg)](Cargo.toml)
[![License](https://img.shields.io/badge/license-BSL--1.1-green.svg)](LICENSE)

Conxian Nexus is a mission-critical supporting interoperability and API facade surface in the Conxian builder platform. It packages lower-level platform capabilities into higher-level external-facing API surfaces, serving as the bridge between raw protocol adapters and developer-facing services.

## Role

This repository exists to package lower-level platform capabilities into higher-level external-facing API or interoperability surfaces where that is useful for builders or partners.

### Ownership & Boundaries

- **Owns**: Higher-level API facade behavior, interoperability service boundaries above direct adapters, and packaged access to lower-level capability surfaces.
- **Does Not Own**: Canonical network adapters, provider-specific integration logic (belongs in `conxian-gateway`), shared-core ownership, or protocol identity.
- **Strategic Role**: Supporting repo.

## Core Features

- **Verifiable State Roots**: Implements a high-performance Merkle Mountain Range (MMR) for O(log N) state proof generation and cryptographic validation.
- **Sovereign Persistence**: Multi-layered persistence logic using Kwil and Tableland for decentralized relational state commitments.
- **FSOC Sequencer**: Specialized sequencer logic for FSOC (Financial Services Operations Center) to mitigate MEV and front-running risks.
- **API Facade**: Provides a clean, standardized REST API for state proofs, metrics, system health, and institutional settlement triggers.
- **Security Hardening**: Includes "Safety Mode" (Sovereign Handoff) triggered by sync drift and mandatory zero-secret logging.

## Technology Stack

- **Language**: [Rust](https://www.rust-lang.org/) (Edition 2021)
- **Web Framework**: [Axum](https://github.com/tokio-rs/axum)
- **Database**: [PostgreSQL](https://www.postgresql.org/) (via [SQLx](https://github.com/launchbadge/sqlx))
- **Caching**: [Redis](https://redis.io/)
- **Infrastructure**: Docker & Docker Compose

## Getting Started

### Prerequisites

- Rust (latest stable)
- Docker & Docker Compose
- PostgreSQL & Redis (local or via Docker)

### Quick Start

1. **Clone & Setup**:
   ```bash
   git clone https://github.com/Conxian/conxian-nexus.git
   cd conxian-nexus
   cp .env.example .env
   ```
2. **Database Migrations**:
   ```bash
   # Ensure DATABASE_URL is set in .env
   cargo install sqlx-cli
   sqlx migrate run
   ```
3. **Run the Application**:
   ```bash
   cargo run
   ```
4. **Verify Installation**:
   ```bash
   curl http://localhost:3000/v1/status
   ```

### Running Tests

```bash
cargo test
./scripts/check_production_boundary.sh
```

## Documentation Hub

- [Contributing Guidelines](CONTRIBUTING.md)
- [Security Policy](SECURITY.md)
- [License](LICENSE)
- [Changelog](CHANGELOG.md)
- [Architecture & PRD](docs/PRD.md)
- [API Specification (OpenAPI)](docs/openapi.yaml)

## Relationship to the Portfolio

- [`lib-conxian-core`](https://github.com/Conxian/lib-conxian-core): Shared capability interfaces and safety primitives.
- `conxian-gateway`: Canonical network and provider adapters.
- `conxius-enclave-sdk`: Secure signer and device trust abstractions.
- `conxius-platform`: Strategic runtime and validation environments.

---
© 2026 Conxian Foundation. All rights reserved.
