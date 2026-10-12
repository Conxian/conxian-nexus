# Conxian Nexus

Conxian Nexus is a universal chain node and proof layer, serving as the primary observation, synchronization, and verification point for Tier 1 Chain Families in the Conxian ecosystem.

## Open Protocol (.org) vs. Commercial Infrastructure (.com) Architectural Boundary

Conxian operates on a strict architectural boundary separating open-source public goods (`conxian.org`) from commercial enterprise offerings (`conxian-labs.com`):

- **Open Core (.org):** Focused on tokenless Bitcoin L1 anchoring, Rust-based Zero-Knowledge state verification (Groth16, SP1 & RISC Zero zkVMs), stateless multi-chain routing, and open-source hardware security SDKs (Android StrongBox TEE). Zero native tokens or speculative tokenomics.
- **Commercial Enterprise (.com):** Focused on SLA-backed managed infrastructure, high-throughput ISO 20022 financial gateway translation middleware, rate-limiting & API authorization, and traditional finance (TradFi) compliance bridges.

For the full system architecture specification, see [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md).

## Purpose

Provide a verifiable synchronization, ordering, and proof layer for Conxian services. Nexus ensures authoritative off-chain state remains aligned with multi-chain activity across Tier 1 chain families (Bitcoin, EVM, Cosmos, Solana, Sui, Aptos) via cryptographic state root commitments (MMR) and multi-chain verification adapters.

## Status

**Active development (v0.4.23).** Production intent exists. Nexus is hardened for Tier 1 multi-chain verification as per ADR-006 / NIP-005 Phase 2, 2026 zkVM proving environments (SP1 and RISC Zero), BitVM3 garbled-circuit fraud proofs, FROST threshold signatures, OP_CAT covenants, DLC oracle attestation, x402 V2 settlement rails, ISO 20022 financial gateway translation, and the SRL-1 Lightning resilience layer.

## Audience

- **Infrastructure Operators:** Deploying high-availability synchronization and verification nodes.
- **Protocol Developers:** Building applications that require deterministic, cross-chain event feeds and state proofs.
- **Security Auditors:** Verifying the integrity of off-chain state commitments against L1 finality.

## Scope

This repository owns the universal chain verification implementation, multi-chain state normalization, and verifiable service interfaces. It is a core protocol component.

## Governance relation

Maintained by Conxian-Labs as public infrastructure. It provides the proof baseline for the Conxian Gateway and public application clients, maintaining strict boundary rules between observation and execution.

## Relationship to the Conxian stack

- **Core Protocol:** `Conxian` (DAO/On-chain) <-> **Nexus** (Observation/Proof Target A)
- **Middleware:** `conxian-gateway` (Transport/ISO 20022 Translation Target B)
- **Hardware Enclave:** `conxius-enclave-sdk` (StrongBox TEE Key Isolation Target C)
- **Orchestration:** `conxius-platform` (Managed SLA & Dashboard Target D)

## Architecture

Nexus is designed as a modular "Glass Node" that provides a verifiable synchronization layer between Layer 1 blockchains and the Conxian ecosystem.

- **Verifiable Proofs**: Generates MMR state root commitments for off-chain state and executes cryptographic verifiers for 2026 zkVMs (SP1 / RISC Zero), zero-knowledge circuits (ZKCP), FROST threshold signatures, OP_CAT covenants, DLC oracle attestations, and x402 V2 settlement rails.
- **Multi-Chain Adapters**: Standardized normalization and cryptographic state verification for Bitcoin (UTXO, BitVM2, BitVM3 garbled circuits, Stacks/sBTC, Fedimint), EVM (Merkle Patricia Trie Keccak-256 root matching), Cosmos (Tendermint IBC SHA-256 header digests), Solana (Ed25519/slot verification), Sui (BCS transaction effects & Move objects), and Aptos (Jellyfish Merkle Tree sparse state proofs) (see [ADR-006](./docs/ADR-006_Tier1_Chain_Families.md) and [NIP-005](./docs/NIP-005_Real_MultiChain_Verification.md)).
- **Resilience & Storage**: Integrated SRL-1 recovery layer for Lightning Network reliability and Neon PostgreSQL IdempotencyStore transactional locks.

## Modules

- `nexus-sync`: Multi-chain ingestion and reorg handling (Bitcoin, EVM, Cosmos, Solana, Sui, Aptos).
- `nexus-state`: MMR state root commitments, IdempotencyStore Neon PostgreSQL transactional locking, and persistence.
- `nexus-executor`: Multi-chain protocol adapters (BitVM2, BitVM3, EVM MPT, Cosmos IBC, Solana, Sui, Aptos, Fedimint, Stacks/sBTC) and sequencing logic.
- `nexus-verification`: Cryptographic verifiers for 2026 zkVMs (SP1 & RISC Zero), FROST Threshold Signatures (CON-1302), OP_CAT Covenants (CON-1303), ZKCP Pre-Image Circuits (CON-1313), DLC Oracle Attestations (CON-803), and x402 V2 Settlement Rails (CON-804).
- `nexus-safety`: Drift monitoring, Hardware Enclave Attestation verification (Hole 2.1), and SRL-1 resilience layer.
- `api`: REST, ISO 20022 Gateway, and gRPC surfaces for multi-chain proofs, settlement verification, identity resolution, and event feeds.

## Documentation

Comprehensive documentation is available at [docs.conxian-labs.com/nexus](https://docs.conxian-labs.com/nexus) (GitHub Pages route).

- [Architecture & System Boundaries](./docs/ARCHITECTURE.md)
- [Operator Guide](./docs/PRD.md)
- [API Reference](./docs/openapi.yaml)
- [Security Model](./SECURITY.md)
- [Observability & Runbooks](./docs/remediation/OBSERVABILITY_RUNBOOK.md)

## Getting started

### Prerequisites

- Docker and Docker Compose
- Rust 1.98.1 (MSRV), PostgreSQL 15, and Redis 7

### Setup & Local Development

1.  **Environment Configuration**:
    Copy the example environment file and configure your secrets:
    ```bash
    cp .env.example .env
    ```
    *Note: Ensure `DATABASE_URL` and `REDIS_URL` are correctly set for your local or docker environment.*

    The Oracle worker is disabled by default. Enabling it with `ORACLE_ENABLED=1`
    also requires `ORACLE_ENDPOINT_URL`, `ORACLE_CONTRACT_PRINCIPAL`, and a signer
    in `CONXIAN_PRIVATE_KEY_HEX` (or the legacy `NEXUS_PRIVATE_KEY` alias). Nexus
    refuses to start the Oracle worker without that signer; it never generates an
    ephemeral production key. Other signing APIs likewise require an explicitly
    configured signer when invoked.

2.  **Database Migrations**:
    Nexus requires a PostgreSQL database. Apply migrations using `sqlx`:
    ```bash
    cargo install sqlx-cli --no-default-features --features postgres
    sqlx migrate run
    ```

3.  **Local Build & Test**:
    Build the workspace binaries and execute the unit and integration test suite:
    ```bash
    cargo build --workspace
    cargo test --workspace
    ```

4.  **Running the Node**:
    Start the Nexus node directly using Cargo:
    ```bash
    cargo run
    ```

5.  **Verifying Service Health & Proof APIs**:
    Probe the local REST API server to verify node status and multi-chain verification endpoints:
    ```bash
    # Node health check
    curl -f http://localhost:8080/health

    # 2026 zkVM Proving Environment Verification (SP1 / RISC Zero)
    curl -X POST http://localhost:8080/v1/verify/zkvm \
      -H "Content-Type: application/json" \
      -d '{"environment": "sp1", "program_digest": "...", "proof": "...", "public_values": "...", "expected_state_commitment": "..."}'

    # ISO 20022 Financial Message Gateway Translation
    curl -X POST http://localhost:8080/v1/gateway/iso20022/translate \
      -H "X-API-Key: cx_live_ent_sample" \
      -H "Content-Type: application/json" \
      -d '{"message_type": "pacs_008", "format": "xml", "raw_message": "...", "target_chain_family": "bitcoin_utxo"}'

    # ZKCP Pre-Image Circuit Verification
    curl -X POST http://localhost:8080/v1/verify/zkcp \
      -H "Content-Type: application/json" \
      -d '{"proof": "...", "public_inputs": "..."}'

    # FROST Threshold Signature Verification
    curl -X POST http://localhost:8080/v1/verify/frost \
      -H "Content-Type: application/json" \
      -d '{"group_public_key": "...", "signature": "...", "message": "..."}'
    ```

### Quick Start (Docker)

If you prefer using Docker, you can start the entire stack (including Postgres and Redis) with:

```bash
docker-compose up --build
```

For more detailed setup instructions, including production hardening, see the [Operator Guide](./docs/PRD.md).

## Policies & Release Guidance

- [CONTRIBUTING.md](./CONTRIBUTING.md)
- [SECURITY.md](./SECURITY.md)
- [SUPPORT.md](./SUPPORT.md)
- [RELEASE.md](./docs/RELEASE.md)
- [CHANGELOG.md](./CHANGELOG.md)
- [CODEOWNERS](./.github/CODEOWNERS)
- [REPO_OWNERSHIP.md](./docs/REPO_OWNERSHIP.md)
- [LICENSE](./LICENSE)

## Support Expectations & Escalation SLA Matrix

As a sovereign, non-custodial infrastructure component, Conxian Nexus operates on a fail-closed model. Public support expectations and escalation paths are strictly codified as follows:

| Severity Level | Definition | Response Target | Resolution Target | Escalation Path |
|---|---|---|---|---|
| **L3 - Critical** | Network downtime, TEE/Enclave attestation failures, or potential security vulnerabilities. | < 2 Hours | < 12 Hours | Email to `security@conxian-labs.com` / PGP-encrypted dispatch to lead maintainers. |
| **L2 - Major** | Multi-chain adapter synchronization drift (> 2 blocks) triggering Safety Mode, or SRL-1 payment recovery blocks. | < 6 Hours | < 24 Hours | Technical Issue template on GitHub / routing via `@Conxian/security-team`. |
| **L1 - Minor** | Local configuration, documentation ambiguity, non-breaking CLI issues, or API telemetry updates. | < 24 Hours | < 48 Hours | General Support template on GitHub / routing to community maintainers. |

All issues are routed and prioritized according to the administrative registry in [CODEOWNERS](./.github/CODEOWNERS). Technical compliance is managed under the policies described in [CONTRIBUTING.md](./CONTRIBUTING.md) and [SECURITY.md](./SECURITY.md).

## Contact

- Support: [support@conxian-labs.com](mailto:support@conxian-labs.com) (See [SUPPORT.md](./SUPPORT.md) for details)
- Security: [security@conxian-labs.com](mailto:security@conxian-labs.com) (See [SECURITY.md](./SECURITY.md) for details)

## License

BUSL 1.1
