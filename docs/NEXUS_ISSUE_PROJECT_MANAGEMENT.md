# Conxian Nexus: GitHub Issues, Projects (v2) & Knowledge Base (KB) Management Framework

This document establishes the standard operating procedures, issue structures, GitHub Projects (v2) workflows, and Knowledge Base (KB) practices for managing production readiness, operational issue mitigation across connected decentralized systems, and commercial business opportunities within the **Conxian Nexus** ecosystem.

---

## 1. Governance & Management Philosophy

Conxian Nexus acts as the zero-token Glass Node observation and cryptographic verification engine for Bitcoin L1, EVM, Cosmos, Solana, Sui, Aptos, and connected decentralized networks. To maintain strict software quality, high-throughput reliability, and rapid issue resolution:

1. **Single Source of Truth**: GitHub Issues and GitHub Projects (v2) serve as the central tracking engine for bugs, feature enhancements, production readiness gaps, and partner onboarding.
2. **Knowledge Base Synchronization**: Technical research, architecture decisions (ADRs), and operational runbooks maintained in `docs/` serve as the Knowledge Base (KB). Issue templates and job cards must explicitly reference KB documents.
3. **Fail-Closed Escalation**: Issues impacting production readiness or cryptographic verification safety trigger automated priority escalation (P0/P1) and CODEOWNERS routing.

---

## 2. GitHub Issue Classification & Standard Workflow

All tasks and reported issues are categorized into five standard issue tracks:

| Track | Issue Type | Primary Focus | GitHub Label | Template / Form |
| :--- | :--- | :--- | :--- | :--- |
| **Track 1** | **Bug Report** | System failures, RPC verification errors, memory/latency regressions | `bug`, `remediation` | `bug_report.yml` |
| **Track 2** | **Production Readiness Gap** | Missing verification adapters, enclave attestation, storage boundary compliance | `production-readiness`, `p0`, `p1` | `repo_baseline_review.yml` |
| **Track 3** | **Decentralized System Issue** | Upstream chain liveness halts, RPC re-orgs, validator set shifts, proof mismatches | `decentralized-system`, `adapter` | `bug_report.yml` |
| **Track 4** | **Business & Partner Job Card** | Enterprise integration, x402 V2 agent rails, ISO 20022 translation, bounty cards | `bounty`, `cxn-job-card`, `business` | `cxn_bounty_job_card.yml` |
| **Track 5** | **Governance & Policy** | CODEOWNERS updates, MSRV upgrades, license compliance, security policies | `governance`, `security` | `governance_request.yml` |

---

## 3. GitHub Projects (v2) Board Architecture

All issues across `conxian-nexus` and linked Conxian ecosystem repositories (`lib-conxian-core`, `conxian-gateway`, `conxian-business`) are managed through a unified **Conxian Nexus Production & Expansion** GitHub Project (v2) board.

### 3.1 Project Board Views

1. **Production Readiness Roadmap View (Table)**:
   - Tracks open gaps from `docs/PRODUCTION_ENABLEMENT_RESEARCH_2026-09-11.md`.
   - Columns: `Title`, `Track`, `Priority (P0, P1, P2)`, `Status`, `Target Release`, `Assignees`, `Linked KB Doc`.

2. **Decentralized System Support View (Kanban)**:
   - Manages verification adapter health and operational issue support across connected networks (Bitcoin, EVM, Cosmos, Solana, Sui, Aptos, Near, XRPL, Starknet, Monad).
   - Columns: `Triage` -> `In Research` -> `In Development` -> `In Verification` -> `Active in Production`.

3. **Commercial & Business Job Cards View (Board)**:
   - Tracks partner integrations, x402 agent settlement rails, ISO 20022 gateway onboarding, and developer bounties.
   - Columns: `Backlog` -> `In Review` -> `In Progress` -> `Testing` -> `Completed`.

---

## 4. Supporting Operational Issues in Decentralized Systems

When an issue occurs in an external decentralized network (e.g. an EVM chain RPC fork, Cosmos IBC client expiration, or Solana slot skip), the following issue management & resolution protocol must be followed:

### Step 1: Incident Issue Creation
Create a GitHub issue using the `bug_report.yml` template with the tag `decentralized-system` and specify:
- Affected Decentralized System & Chain ID.
- Observed Behavior (e.g. state proof verification failure, unverified header digest).
- Impact Assessment (e.g. Fail-closed rejection rate, RPC fallback status).

### Step 2: Automated Routing & Triage
- Issues labeled `p0` or `decentralized-system` automatically notify assigned CODEOWNERS.
- The issue is linked to the GitHub Project board under `Decentralized System Support`.

### Step 3: Mitigation & KB Update
- Apply mitigation in code (e.g. fail-closed protection, multi-RPC quorum verification, re-org invalidation handling).
- Document root cause analysis and update the Knowledge Base in `docs/RESEARCH.md` or `docs/CHAIN_SUPPORT_BOUNDARIES.md`.

---

## 5. Knowledge Base (KB) Management & Maintenance

The `docs/` directory forms the repository's living Knowledge Base. To keep the KB aligned with active development:

1. **Mandatory Issue Reference**: Every non-trivial PR or issue must link to a specific section in `docs/` (e.g., `docs/PRODUCTION_ENABLEMENT_RESEARCH_2026-09-11.md` Section 2).
2. **ADR & Research Updates**: Architectural shifts or new chain adapter specifications must be committed to `docs/` before or alongside code implementation.
3. **Continuous Review**: Quarterly reviews synchronize `docs/GAP_ANALYSIS.md`, `README.md`, and issue template forms with current codebase state.
