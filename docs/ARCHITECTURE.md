# Nexus architecture and operating boundary

## Role

Conxian Nexus is the evidence and verification service. It observes configured networks, produces deterministic state commitments, and verifies proofs against explicit Bitcoin state bindings. It does not own banking-message translation, customer tenancy, signing keys, transaction custody, or enterprise deployment policy.

## Public protocol and commercial operation

| Surface | Public protocol responsibility | Managed Conxian-Labs responsibility |
|---|---|---|
| State observation | Deterministic adapters and evidence schemas | Operated data sources, capacity, monitoring, and incident response |
| Proof verification | Fail-closed verifier interfaces and Bitcoin commitment binding | Audited verifier builds, rollout control, availability, and support |
| API | Portable proof and health contracts | Authentication at the service perimeter, quotas, observability, and SLA reporting |
| Persistence | Reproducible schema and integrity rules | Backups, replication, retention, recovery, and regional placement |

The code is currently BUSL-1.1 source-available. The intended public-good role does not by itself make the repository open source; changing that status requires an explicit license decision.

Commercial ISO 20022 translation, API-key tenancy, banking compliance, and routing policy are owned by `conxian-gateway`. Multi-tenant cloud orchestration is owned by `conxius-platform`. Hardware-backed key isolation and value-bearing signing are owned by `conxius-enclave-sdk`.

## Invariants

1. Bitcoin L1 is the root of verification for promoted production claims.
2. Conxian has no native token, staking asset, or speculative tokenomics.
3. Structural validation is not cryptographic proof verification.
4. A proof succeeds only after an explicitly configured cryptographic backend verifies it and the public values bind to the expected Bitcoin commitment.
5. Missing providers, stale observations, malformed evidence, and unsupported proof systems fail closed.
6. Nexus does not custody keys or execute banking settlement.

## zkVM provider boundary

`src/verification/zkvm.rs` defines a bounded, versioned envelope for SP1 and RISC Zero receipts. It validates encoding, payload size, program/image identifiers, and a canonical Bitcoin state commitment. The `ZkVmProofBackend` trait is the integration point for audited verifier adapters.

`UnavailableZkVmBackend` is the default scaffold and always returns an error. No REST verification route is published until a real backend, conformance fixtures, and deployment evidence exist. This prevents arbitrary bytes from being reported as verified proofs.

A production adapter must:

- invoke the upstream verifier for the selected zkVM;
- bind the verified journal/public values to the exact bytes Nexus validated;
- pin and report the verifier implementation and version;
- enforce program/image allowlists through reviewed configuration;
- return a typed rejection for unsupported or malformed receipts;
- pass upstream-generated valid and tampered proof fixtures.

## Legacy network adapters

Stacks and other non-Bitcoin adapters currently remain compatibility observation modules. They are not Conxian trust roots and do not create a native asset. Removal or isolation requires fixture-backed migration so historical evidence and consumers are not silently broken. New protocol authority must not be added to those adapters.

## Deployment gate

Nexus is not a valid control-plane admin runtime until its admin API provides the canonical list routes, durable mutation/audit persistence, truthful readiness and synchronization data, and authenticated read/write separation expected by `conxian-business`. Provider deployment must remain gated until those contracts pass end-to-end tests against the deployed service.
