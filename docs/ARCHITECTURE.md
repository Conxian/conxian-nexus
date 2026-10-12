# Conxian System Architecture & Enterprise Boundary Specification

## Overview

Conxian is an enterprise-grade financial technology and blockchain protocol stack designed around a strict architectural boundary between the **Open Protocol (.org)** public goods layer and the **Commercial Enterprise (.com)** infrastructure offerings.

```
+-----------------------------------------------------------------------------------+
|                                 CONXIAN ECOSYSTEM                                 |
+-----------------------------------------------------------------------------------+
|                                                                                   |
|    .org (Open Core / Public Protocol)            .com (Commercial Enterprise)     |
|   -----------------------------------           ------------------------------    |
|   - Tokenless L1 Proof Anchor                   - Managed High-Availability Nodes |
|   - Rust-Based ZK State Verification            - ISO 20022 Financial Gateway     |
|   - Stateless Multi-Chain Routing               - SLA & Managed API Rate Limits   |
|   - Android StrongBox TEE Hardware SDK          - Enterprise Security Auditing    |
|                                                                                   |
+-----------------------------------------------------------------------------------+
```

---

## Strategic Boundaries & Repository Execution Targets

### Target A: conxian-nexus (The Verification Engine)
- **Role:** Primary observation, synchronization, and zero-knowledge proof verification layer rooted in Bitcoin L1.
- **Open Core (.org):**
  - Universal ZK state verification across Tier 1 chains (Bitcoin, EVM, Cosmos, Solana, Sui, Aptos).
  - Native support for modern 2026 zkVM proving environments (**SP1** and **RISC Zero**).
  - Stateless execution, deterministic MMR state root commitments, and zero tokenomics.
- **Commercial Enterprise (.com):**
  - SLA-backed managed Nexus node deployments with automatic failover and high-availability load balancing.

### Target B: conxian-gateway (The Commercial Bridge)
- **Role:** Financial middleware bridging traditional institutional systems to universal blockchain routing.
- **Open Core (.org):**
  - Open-source, stateless message transformation primitives for ISO 20022 (pacs.008, pain.001, camt.053 XML/JSON).
- **Commercial Enterprise (.com):**
  - High-throughput banking compliance translation, API key rate-limiting middleware, audit logging, and automated reconciliation for enterprise banking rails.

### Target C: conxius-enclave-sdk (Hardware Security)
- **Role:** Hardware-enforced key isolation and non-custodial transaction signing.
- **Open Core (.org):**
  - Android StrongBox TEE cryptographic integration SDK for offline key protection.
- **Commercial Enterprise (.com):**
  - White-glove institutional security integration, hardware security module (HSM) attestation auditing, and bespoke enterprise deployment support.

### Target D: conxius-platform (Enterprise Orchestration)
- **Role:** Managed deployment infrastructure and cloud service management.
- **Open Core (.org):**
  - Containerization standards and local deployment templates (Docker Compose).
- **Commercial Enterprise (.com):**
  - Multi-tenant cloud orchestration dashboards, automated multi-cloud CI/CD pipelines (AWS/GCP), and SLA compliance monitoring.

---

## Core System Invariants

1. **Zero-Token Policy:**
   The entire Conxian ecosystem is strictly anchored to **Bitcoin L1**. No native protocol tokens, speculative staking, or tokenomic logic exist anywhere in the protocol stack.

2. **Fail-Closed Security Model:**
   All cryptographic verification modules (Groth16, FROST, OP_CAT, ZKCP, zkVM SP1/RISC Zero, x402 V2) fail closed upon structural or verification anomaly.

3. **Multi-Chain Universal Verification:**
   Stateless cryptographic proof checking across Tier 1 chain families without requiring custody or custodial bridges.
