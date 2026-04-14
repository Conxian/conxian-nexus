# Conxian Portfolio Repository Inventory [CON-410]

This document provides the canonical inventory of all repositories and subrepositories within the Conxian ecosystem, mapped by business unit and release criticality.

## 1. Core Protocol & Execution
- **Conxian/Conxian**: Main protocol and smart contract repository. (Criticality: P0)
- **lib-conxian-core**: Shared cryptographic, wallet, and service primitives. (Criticality: P0)
- **conxian-nexus**: Glass Node and state commitment layer. (Criticality: P0)
- **conxian-gateway**: Institutional rail and compliance bridge. (Criticality: P0)

## 2. Wallet & Client Interface
- **conxius-wallet**: Sovereign HD wallet for mobile and web. (Criticality: P0)
- **Conxian_UI**: Web-based dashboard and business portal. (Criticality: P1)
- **lib-conclave-sdk**: B2B SDK for external integration. (Criticality: P1)

## 3. Platform & Operations
- **conxius-platform**: Orchestration and deployment automation. (Criticality: P1)
- **conxian-business**: Business operations, GTM, and administrative material. (Criticality: P1)
- **conxian-labs-site**: Public-facing website and narrative. (Criticality: P2)

## 4. Supporting Repositories
- **stacksorbit**: Historical and legacy support. (Criticality: P3)
- **.github**: Org-wide governance, templates, and standards. (Criticality: P0)

## Release Standard (v0.5.0)
All repositories marked as P0 and P1 must adhere to the **Mainnet-Only Production Boundary**:
- `main`: Mainnet-only production code.
- `staged`: Promotion and mainnet validation branch.
- `dev`: Testnet-only development.
