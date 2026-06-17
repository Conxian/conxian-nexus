# Remediation: Clarity 4 Verification Gap (CON-1200)

## Issue
The repository documents a toolchain gap between the current Clarity 4 contract code and local verification support. Production readiness requires a validated path for compilation, simulation, and release verification.

## Findings
- **Clarity 4 Primitives**: Clarity 4 introduces on-chain contract verification (`contract-hash?`), asset post-conditions (`restrict-assets?`), and `secp256r1-verify` for passkey-based authentication.
- **Toolchain Status**: `clarinet` is the standard toolchain for Stacks developers. Latest documentation confirms support for Clarity versions up to 4.
- **Local Nexus Integration**: Nexus currently uses `lib-conxian-core`'s `ContractBridge` to interact with Stacks. The local gap appears to be the absence of automated `clarinet` execution in CI to verify the contracts before deployment.

## Remediation Plan
1. **Toolchain Alignment**: Standardize on `clarinet` v2.x for all local contract verification.
2. **CI Guardrail**: Add a `clarinet check` step to the GitHub Actions workflow to ensure Clarity 4 contracts are valid against the current node configuration.
3. **Simulation Harness**: Expand integration tests in `tests/stacks_adapter_test.rs` to exercise the new Clarity 4 primitives (especially `contract-hash?`) via a local devnet or mock bridge.

## Verification Checklist
- [ ] `clarinet check` passes locally.
- [ ] CI workflow updated and green.
- [ ] Contract hash verification tested in simulation.
