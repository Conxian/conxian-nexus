# BIP-110 Alignment — Phase 1

## Status and scope

BIP-110 is a published Bitcoin Improvement Proposal. Publication is not
deployment, activation, adoption, or evidence that the Bitcoin network
currently enforces the proposal. This repository therefore treats BIP-110 as
a proposal to align against, not as an active consensus rule. The Phase 1
implementation is intentionally named an **observed-size policy assessment**;
it does not claim BIP-110 compliance, consensus validity, Bitcoin sync, or SPV
security.

Phase 1 implements a pure function over caller-supplied metadata in
`src/sync/bip110.rs`. It assesses these four proposed size boundaries:

| Observed metadata | Limit | Phase 1 rule label |
| --- | ---: | --- |
| OP_RETURN output script | 83 bytes | `op_return_script` |
| Other output scriptPubKey | 34 bytes | `non_op_return_script_pubkey` |
| OP_PUSHDATA* payload | 256 bytes | `pushdata` |
| Witness element | 256 bytes | `witness_element` |

OP_RETURN scripts and non-OP_RETURN scriptPubKeys are supplied in separate
vectors. This is deliberate: an OP_RETURN script may be larger than 34 bytes
while remaining within the separate 83-byte OP_RETURN observation limit. Each
input element is assessed independently, and all violations are returned in a
deterministic rule order followed by input order.

An unavailable observation is classified as `unknown`, with no claim that it
is within the limits. The implementation does not parse raw transactions,
infer omitted metadata, connect to a Bitcoin network, or provide an observation
backend. The backend availability gauge is therefore initialized to `0`.

## Explicit limitations

This phase does **not** implement:

- Bitcoin consensus validation or a transaction/block validity decision;
- BIP-110 deployment, activation state, expiry, or grandfathering of existing
  UTXOs;
- undefined witness-version rules, witness-version semantics, Taproot annex or
  control-block rules, `OP_SUCCESS*`, `OP_IF`, or `OP_NOTIF` execution rules;
- raw transaction parsing, script execution, or complete script semantics;
- block-header validation, connected-header validation, or proof-of-work;
- Merkle transaction inclusion proofs, compact filters, or peer protocols; or
- a completeness guarantee that caller-supplied metadata covers every relevant
  field.

These limitations are also part of the Rustdoc for the public module. The
classification `within_observed_size_limits` means only that the supplied
sizes were at or below the four Phase 1 limits. It is not a consensus result.
`exceeds_observed_size_limits` means that one or more supplied sizes exceeded a
limit. `unknown` means the required observation was unavailable.

## Core and SDK comparison

Bitcoin Core is a C++ full-node implementation with transaction/block
validation and chainstate machinery. Its current source and AssumeUTXO design
are useful architectural references, but Nexus does not embed Bitcoin Core and
does not currently have a native Bitcoin backend. The current Nexus dependency
set also contains no Bitcoin Core or Bitcoin protocol validation library.

The candidate SDK path reviewed for this issue is design-only and incomplete
for the required parser, consensus, deployment, and proof surfaces. It is not
used as an implementation dependency or as evidence of network support.

The newer Core validator path considered in the research is unavailable at
Nexus's current API/MSRV boundary. No dependency upgrade was made because it
would expand Phase 1 into a validator/backend integration, risk the repository's
Rust 1.82 MSRV and legacy APIs, and still would not supply the missing
deployment, proof, or backend boundary. The small pure policy module keeps the
current dependency graph and makes the unsupported boundary explicit.

## Metrics and observability

`src/metrics.rs` registers fixed-cardinality metrics in the default Prometheus
registry. Registration initializes every fixed label value even when no
observation has been recorded, so a scrape exposes zero-valued series.

| Metric | Type | Labels / values |
| --- | --- | --- |
| `nexus_bip110_observations_assessed_total` | counter | `classification`: `within_observed_size_limits`, `exceeds_observed_size_limits`, `unknown` |
| `nexus_bip110_observed_size_violations_total` | counter | `rule`: `pushdata`, `op_return_script`, `non_op_return_script_pubkey`, `witness_element` |
| `nexus_bip110_observation_backend_available` | gauge | no labels; `0` until a future backend is wired, `1` only when explicitly set |

No metric label contains a transaction ID, block hash, address, height, peer,
payload, or arbitrary error. Assessment remains pure; recording metrics is an
explicit separate operation. The read-only REST endpoint is `GET /metrics` and
uses Prometheus text exposition. `/v1/analytics/metrics` remains the separate
STX/Postgres analytics endpoint.

## Current and future architecture boundary

The following table distinguishes source-backed protocol capabilities from
Nexus design inference.

| Boundary | Source-backed statement | Nexus implication |
| --- | --- | --- |
| Headers and proof-of-work | BIP-37 describes SPV clients checking connected headers and relying on proof-of-work while not fully validating the chain. | **Inference:** a future light-verification boundary must validate the header chain and proof-of-work before treating any transaction evidence as anchored. Phase 1 does neither. |
| Merkle inclusion | BIP-37 specifies partial Merkle branches and checks the computed root against the block header. | **Inference:** future transaction evidence needs a separately verified inclusion proof; size assessment alone is not inclusion. |
| BIP-157/BIP-158 | BIP-157 and BIP-158 specify compact-block-filter serving and client-side filter use for light clients. | **Inference:** compact filters can reduce candidate downloads, but they do not replace header, proof-of-work, transaction, or inclusion verification. |
| Pruned full node / AssumeUTXO | Bitcoin Core documents AssumeUTXO and maintains chainstate/validation components in the reference client. | **Inference:** a future deployment could use a pruned full node or an AssumeUTXO-backed node as an external full-validation boundary. Nexus currently has neither. |
| Utreexo | The Utreexo paper and project describe a dynamic accumulator for the Bitcoin UTXO set. | **Inference:** Utreexo is an experimental future research option here, not a Phase 1 dependency or a production security claim. |

Nexus's existing Stacks synchronization path must not be described as native
Bitcoin full-node synchronization. This BIP-110 slice observes no Bitcoin
network and introduces no Bitcoin backend.

## Future light-verification benchmark matrix

The following matrix is a concrete gate for a later light-verification phase;
it is not implemented by Phase 1.

| Case | Benchmark input | Required result |
| --- | --- | --- |
| Header chain | 1,000 / 10,000 / 100,000 connected headers with valid proof-of-work | Deterministic acceptance with measured throughput, bounded memory, and no external service dependency in the verifier test |
| Header linkage | One wrong previous-header hash at positions 1, 500, and the final header | Deterministic rejection naming the first invalid linkage |
| Proof-of-work | Valid headers plus one target/nonce mutation at each benchmark size | Deterministic rejection; no invalid chain accepted |
| Merkle inclusion | Inclusion proofs at first, middle, and last transaction positions, plus wrong-root and wrong-branch cases | Valid proofs accepted only when the calculated root matches the verified header; malformed or mismatched proofs rejected |
| BIP-157/158 filter flow | Matching, non-matching, and deliberate false-positive compact-filter cases | False positives trigger a transaction download; filters never serve as an inclusion proof or as a reason to accept an unverified transaction |
| BIP-110 metadata | All four exact-boundary and one-above cases, multiple violations, unavailable metadata, and proposal-specific grandfathering fixtures | Phase 1 classifications remain deterministic; future consensus work must separately model grandfathering and all unimplemented rules |
| Taproot and witness | Key-path, script-path, annex, control-block, witness-version, and script-execution fixtures | Rules are evaluated by a dedicated consensus/script component, not inferred from Phase 1 sizes |
| Pruned/AssumeUTXO boundary | Same block/transaction corpus checked through the chosen full-node boundary and the light verifier | Results agree; snapshot trust and background validation state are explicit and observable |
| Utreexo research | Accumulator membership and spend-proof corpus, if selected later | Proof verification is independently benchmarked and never silently substituted for header or transaction validation |

Acceptance criteria for that future phase are:

1. No invalid header, proof-of-work, Merkle proof, filter workflow, or
   transaction is accepted in negative fixtures.
2. Every positive fixture has an independently reproducible expected result
   and a stable error category for negative cases.
3. Benchmarks report throughput, peak memory, proof size, and verification
   latency at each matrix size.
4. The trust boundary identifies whether validation came from a fully
   validated chain, an AssumeUTXO snapshot pending background validation, a
   compact-filter candidate, a Merkle proof, or an experimental accumulator.
5. BIP-110 deployment state, grandfathering, and all Taproot/script rules are
   tested separately from the observed-size policy.

## Primary references

These are canonical primary sources. A link to a proposal or design document
is not a claim that the proposal is deployed or adopted.

- BIP-110, *Reduced Data Temporary Softfork*:
  https://github.com/bitcoin/bips/blob/master/bip-0110.mediawiki
- BIP-3, *Updated BIP Process*:
  https://github.com/bitcoin/bips/blob/master/bip-0003.md
- BIP-37, *Connection Bloom filtering*:
  https://github.com/bitcoin/bips/blob/master/bip-0037.mediawiki
- BIP-157, *Client Side Block Filtering*:
  https://github.com/bitcoin/bips/blob/master/bip-0157.mediawiki
- BIP-158, *Compact Block Filters for Light Clients*:
  https://github.com/bitcoin/bips/blob/master/bip-0158.mediawiki
- Bitcoin Core AssumeUTXO design:
  https://github.com/bitcoin/bitcoin/blob/master/doc/assumeutxo.md
- Bitcoin Core validation source:
  https://github.com/bitcoin/bitcoin/blob/master/src/validation.cpp
- Utreexo paper, *A dynamic hash-based accumulator optimized for the Bitcoin
  UTXO set*:
  https://eprint.iacr.org/2019/611
- Original Utreexo project:
  https://github.com/mit-dci/utreexo
