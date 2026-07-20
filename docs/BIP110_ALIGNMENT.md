# BIP-110 Alignment — Phase 1

## Status and scope

BIP-110 is marked `Complete` as a proposal but this is not evidence of
deployment, activation, or adoption. This repository therefore treats BIP-110
as a proposal to align against, not as an active consensus rule. The Phase 1
implementation is intentionally named an **observed-size policy assessment**;
it does not claim BIP-110 compliance, consensus validity, Bitcoin sync, or SPV
security.

Phase 1 implements a pure function over caller-supplied metadata in
`src/sync/bip110.rs`. It assesses these five proposed size boundaries when the
caller supplies typed, serialized sizes:

| Observed metadata | Limit | Phase 1 rule label |
| --- | ---: | --- |
| OP_RETURN output script | 83 bytes | `op_return_script` |
| Other output scriptPubKey | 34 bytes | `non_op_return_script_pubkey` |
| Rule-limited OP_PUSHDATA* payload | 256 bytes | `pushdata` |
| Script-argument witness item | 256 bytes | `script_argument_witness_item` |
| Taproot control block | 257 bytes | `taproot_control_block` |

`ObservedSizeMetadata` contains an explicit availability flag, an explicit
`Complete`/`Incomplete` coverage indicator, and a tagged `ObservedSizeItem`
list. OP_RETURN scripts and non-OP_RETURN scriptPubKeys have separate typed
categories: an OP_RETURN script may be larger than 34 bytes while remaining
within the separate 83-byte OP_RETURN observation limit. BIP16 redeemScript
pushes are explicitly exempt from the modeled `pushdata` rule. Witness scripts
and Tapleaf scripts are explicitly exempt from this partial 256-byte size
check. Script-argument witness items remain subject to 256 bytes, while
Taproot control blocks have their own 257-byte limit. These exemptions only
describe this partial size assessor; they do not imply overall BIP-110 or
consensus validity.

Each input element is assessed independently. Definite violations are returned
in a deterministic rule order followed by their original observation index.

Unavailable, incomplete, empty, or unsupported metadata is classified as
`unknown`, with no claim that it is within the limits. A definite modeled
violation is still returned and classified as `exceeds_observed_size_limits`
even if other metadata is incomplete or unsupported. The implementation does
not parse raw transactions, infer omitted metadata, connect to a Bitcoin
network, or provide an observation backend. The backend availability gauge is
therefore initialized to `0`.

## Explicit limitations

This phase does **not** implement:

- Bitcoin consensus validation or a transaction/block validity decision;
- BIP-110 deployment, activation state, expiry, or grandfathering of existing
  UTXOs;
- undefined witness-version rules, witness-version semantics, Taproot annex,
  tapscript execution, `OP_SUCCESS*`, `OP_IF`, or `OP_NOTIF` execution rules;
- raw transaction parsing, script execution, or complete script semantics;
- block-header validation, connected-header validation, or proof-of-work;
- Merkle transaction inclusion proofs, compact filters, or peer protocols; or
- a completeness guarantee that caller-supplied metadata covers every relevant
  field.

These limitations are also part of the Rustdoc for the public module. The
classification `within_observed_size_limits` means only that non-empty,
available metadata asserted complete coverage, contained no unsupported
category, and every supplied modeled size was at or below its Phase 1 limit. It
is not a consensus result. `exceeds_observed_size_limits` means that one or
more supplied sizes definitely exceeded a modeled limit. `unknown` means the
metadata was unavailable, incomplete, empty, or contained an unsupported
category without a definite modeled violation.

## Core and SDK comparison

Bitcoin Core is a C++ full-node implementation with transaction/block
validation and chainstate machinery. Its current source and AssumeUTXO design
are useful architectural references, but Nexus does not embed Bitcoin Core and
does not currently have a native Bitcoin backend. The current Nexus dependency
set also contains no Bitcoin Core or Bitcoin protocol validation library.

The exact `Conxian/lib-conxian-core` implementation reviewed for comparison is
commit [`5647ade8b0294351946f2e36ea77c43d8edeceed`](https://github.com/Conxian/lib-conxian-core/commit/5647ade8b0294351946f2e36ea77c43d8edeceed),
with the BIP-110 implementation in
[`src/control_model/trust.rs`](https://github.com/Conxian/lib-conxian-core/blob/5647ade8b0294351946f2e36ea77c43d8edeceed/src/control_model/trust.rs)
and module wiring in
[`src/control_model/mod.rs`](https://github.com/Conxian/lib-conxian-core/blob/5647ade8b0294351946f2e36ea77c43d8edeceed/src/control_model/mod.rs).
The implementation commit declares Rust 1.85. Nexus still pins
`lib-conxian-core` to commit
[`3b091d2700d840514427e4190c40d631b6d8132c`](https://github.com/Conxian/lib-conxian-core/commit/3b091d2700d840514427e4190c40d631b6d8132c),
which predates that module. This follow-up does not move the pin: Nexus
declares Rust 1.82 in `Cargo.toml`, and changing the pin would be a dependency
and API/MSRV change outside this conservative metadata-only scope. The locked
workspace is verified with Rust 1.94.0 because its existing `sqlx` dependency
requires that newer compiler; that verification constraint is not a request to
change the declared MSRV.

The exact `Conxian/conxius-enclave-sdk` implementation reviewed for comparison
is commit [`a9986ef104b9cdd560bf7316f38b6878620e1ae5`](https://github.com/Conxian/conxius-enclave-sdk/commit/a9986ef104b9cdd560bf7316f38b6878620e1ae5),
with the implementation in
[`src/protocol/bip110.rs`](https://github.com/Conxian/conxius-enclave-sdk/blob/a9986ef104b9cdd560bf7316f38b6878620e1ae5/src/protocol/bip110.rs),
the `bip110_compliant` feature declaration in
[`Cargo.toml`](https://github.com/Conxian/conxius-enclave-sdk/blob/a9986ef104b9cdd560bf7316f38b6878620e1ae5/Cargo.toml),
the feature-gated module declaration in
[`src/protocol/mod.rs`](https://github.com/Conxian/conxius-enclave-sdk/blob/a9986ef104b9cdd560bf7316f38b6878620e1ae5/src/protocol/mod.rs),
and BIP-322 integration in
[`src/protocol/bip322.rs`](https://github.com/Conxian/conxius-enclave-sdk/blob/a9986ef104b9cdd560bf7316f38b6878620e1ae5/src/protocol/bip322.rs).
That commit also declares Rust 1.85. The SDK path is a design reference only;
it is not a Nexus dependency or evidence of Bitcoin network support.

These exact references make the pin and MSRV boundary reproducible without
claiming that either external implementation supplies Nexus's missing parser,
consensus, deployment, proof, or backend surfaces. The small pure policy module
keeps the current dependency graph and makes the unsupported boundary explicit.

## Metrics and observability

`src/metrics.rs` owns a private Prometheus `Registry` containing only the
intentionally exposed BIP-110 metrics. Registration initializes every fixed
label value even when no observation has been recorded, so a scrape exposes
zero-valued series without colliding with an embedding application's default
registry or exposing unrelated process metrics.

| Metric | Type | Labels / values |
| --- | --- | --- |
| `nexus_bip110_observations_assessed_total` | counter | `classification`: `within_observed_size_limits`, `exceeds_observed_size_limits`, `unknown` |
| `nexus_bip110_observed_size_violations_total` | counter | `rule`: `pushdata`, `op_return_script`, `non_op_return_script_pubkey`, `script_argument_witness_item`, `taproot_control_block` |
| `nexus_bip110_observation_backend_available` | gauge | no labels; `0` until a future backend is wired, `1` only when explicitly set |

No metric label contains a transaction ID, block hash, address, height, peer,
payload, or arbitrary error. Assessment remains pure; recording metrics is an
explicit separate operation. The read-only REST endpoint is `GET /metrics` and
uses Prometheus text exposition from this dedicated registry only. It is
currently unauthenticated, so operators must restrict it to an internal or
otherwise trusted network boundary. It exposes aggregate BIP-110-only metrics,
not per-transaction or per-peer data, and returns HTTP 500 if text exposition
encoding fails. `/v1/analytics/metrics` remains the separate STX/Postgres
analytics endpoint and is not replaced by `/metrics`.

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
| BIP-110 metadata | All five exact-boundary and one-above cases, multiple violations, unavailable metadata, and proposal-specific grandfathering fixtures | Phase 1 classifications remain deterministic; future consensus work must separately model grandfathering and all unimplemented rules |
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
