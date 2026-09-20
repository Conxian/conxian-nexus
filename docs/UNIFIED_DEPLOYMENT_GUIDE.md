# Conxian Ecosystem: Unified Client Deployment & Setup Architecture

## 1. Overview & Client Journey

When a client or enterprise operator purchases and deploys the **Conxian** stack from the Conxian Organization, they are installing a sovereign, non-custodial, multi-chain proof and observation node.

The Conxian ecosystem consists of three primary connected components:
1. **Conxian Nexus (`conxian-nexus`)**: Universal "Glass Node" proof and observation layer. Serves state root commitments (MMR), verifies multi-chain transactions (Bitcoin, EVM, Cosmos, Stacks, Lightning, Fedimint), and manages B2B tier telemetry.
2. **Conxian Gateway (`conxian-gateway`)**: Execution and RPC adaptation layer. Handles client transport, mandate settlement, and transaction submission to L1/L2 networks.
3. **Conxian Enclave SDK (`conxius-enclave-sdk`)**: Hardware Enclave TEST/SGX/SEV confidential execution environment. Provides X.509 DER attestation certificates and secure key derivation.

---

## 2. Client Prerequisites & Inputs

To deploy Conxian Nexus and connected services, the client must configure the following core inputs in their environment (`.env`):

| Variable | Purpose | Client Provided Value |
|---|---|---|
| `DATABASE_URL` | PostgreSQL 15+ database connection string | `postgres://<user>:<pass>@<host>:5432/<db>` |
| `REDIS_URL` | Redis 7+ cache & pub/sub connection string | `redis://:<pass>@<host>:6379` |
| `STACKS_NODE_RPC_URL` | Stacks L1 RPC node endpoint | `https://api.mainnet.hiro.so` (or private node) |
| `NEXUS_ADMIN_API_TOKEN` | Scoped admin bearer API token | Securely generated token (min 32 chars) |
| `ADMIN_PUBLIC_KEYS` | Hex-encoded Secp256k1 public keys for dual-signature release approvals (NIP-004) | Comma-separated hex keys |
| `ERP_ATTESTATION_TRUSTED_KEYS_JSON` | HMAC secrets for SAP/Oracle OData sync | JSON string e.g. `{"key1": "secret"}` |
| `CONXIAN_PRIVATE_KEY_HEX` | Signer private key for Oracle/Settlement workers | 64-char hex private key |
| `NEXUS_BITVM_GROTH16_TRUSTED_REGISTRY_JSON` | BitVM2 Groth16 verifying key registry | JSON string with BN254 verification keys |

---

## 3. End-to-End Installation & Deployment Flow

1. Pull Code & Submodules: `git clone --recursive https://github.com/Conxian/conxian-nexus`
2. Configure Environment: `cp .env.example .env`
3. Database Migration: `sqlx migrate run`
4. Start Infrastructure: `docker-compose up --build -d`
5. Verify Health: `GET /health` & `GET /v1/status`

---

## 4. Connectivity & Verification Matrix

To ensure end-to-end connectivity across client-deployed assets:
- **Nexus <-> PostgreSQL**: Stores MMR state root commitments (`mmr_nodes`), audit logs (`stacks_verified_transactions`, `evm_verified_receipts`, `fedimint_verified_proofs`), and payment intents.
- **Nexus <-> Redis**: Manages API key telemetry, Lightning B2B subscription tier upgrades, and nonce replay prevention.
- **Nexus <-> Hardware Enclave**: Verifies X.509 DER attestation certificates submitted with `ExecutionRequest` payloads.
- **Nexus <-> Conxian Gateway**: Gateway queries state root proofs from Nexus (`/v1/proof`, `/v1/mmr-proof`) before executing downstream mandates.

---

## 5. Architectural Recommendation: Unified Installer (`conxian-cli`)

To streamline client onboarding and eliminate manual multi-step configuration, we recommend deploying a unified installation CLI tool (`conxian-cli` or `conxianup`):

```
# Recommended Unified One-Line Installation:
conxian-cli init --environment production
conxian-cli status
```

The unified installer automatically orchestrates:
1. Prerequisite checks (Rust toolchain, Docker, PostgreSQL, Redis).
2. Automated `.env` validation & secret generation.
3. Database migration execution.
4. Health probe verification across REST (`:3000`), gRPC (`:50051`), and Prometheus metrics (`:3000/metrics`).
