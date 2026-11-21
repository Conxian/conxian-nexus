# ZKML verification soundness

Status: partially remediated on 2026-11-21 (see "What changed"). The remaining
items are listed at the end and are **not** fixed.

## Scope

`POST /v1/zkml/verify` (`src/api/zkml.rs`) is mounted at `/v1/zkml` in
`src/api/rest.rs`. `AGENTS.md` classifies the module as `⚠️ API only`, which is
accurate: the endpoint verifies a caller-supplied Groth16 proof, but Nexus does
not run or prove model inference itself. Nothing here is a machine-learning
implementation; the risk is entirely in how an inference claim is accepted as
verified.

## The trust question

The endpoint answers one question: "did some prover run the agreed inference
circuit over the input I am about to trust?" A useful answer needs three things
to be true at once:

1. the proof was produced for **this** circuit, not a different circuit that
   happens to use the same curve;
2. the proof is over **this** public input (the input commitment the caller
   reports), not some other value;
3. the claimed values are **canonical**, so two different byte strings cannot
   denote the same statement.

Before this change all three could fail, and the endpoint would still return
`{"valid": true}`.

## Defect 1 — cross-protocol proof replay

`verify_zkml_handler` built a `ZkcpProofPayload` with
`circuit_id: ZKCP_CIRCUIT_ID` and verified it through `ZkcpVerifier::verify_proof`.
That verifier compared `payload.circuit_id` against the `ZKCP_CIRCUIT_ID`
constant. Because the handler had just written that same constant into the
payload, the comparison was a constant against itself and always passed.

Consequence: any valid ZKCP pre-image proof — a proof that a SHA-256 pre-image
exists, which is the payment-settlement primitive — was accepted as a ZKML
inference attestation. The two protocols share the BN254 curve and the Groth16
proof format, so the bytes were structurally interchangeable. An attacker with a
legitimate settlement proof could mint an inference attestation for an input
commitment of their choosing.

Fix: the expected circuit identifier is now an argument to the verifier
(`verify_groth16_statement`), and the ZKML handler passes `ZKML_CIRCUIT_ID`.
The value comes from the verifier, not the request.

## Defect 2 — the commitment was never bound to the proof

`verify_proof` decoded `hash_commitment`, checked that it was 32 bytes of hex,
and then never referenced it again. The pairing was checked against
`payload.public_inputs`, which the caller supplies independently.

Consequence: a caller could take any valid proof over public input `P`, report
`hash_commitment = C` for an unrelated `C`, and receive `valid: true`. The
commitment — the thing the attestation is supposed to be about — was decorative.

Fix: `hash_commitment` must appear in `public_inputs`, otherwise the verifier
returns `CommitmentNotBound`.

## Defect 3 — non-canonical public inputs

`Fr::from_be_bytes_mod_order` reduces modulo the BN254 scalar modulus `r`
without complaint. Any 32-byte value `v >= r` therefore denotes `v - r`, and a
32-byte string is not a unique representation of a field element.

Consequence: two distinct 32-byte commitments could map to the same field
element, so a proof over one could be replayed for the other. This is the
classic malleable-public-input trap; the reduction is silent and the resulting
`valid: true` looks identical.

Fix: each public input is round-tripped through `into_bigint().to_bytes_be()`
and rejected with `NonCanonicalPublicInput` if it does not reproduce the input
bytes exactly.

## Defect 4 — attestation identity was random

The handler returned `format!("zkml_attest_{}", uuid::Uuid::new_v4())`. A random
identifier is not bound to anything, so the same proof re-submitted produced
unlimited distinct attestations, and an attestation could not be checked against
the statement it claimed to certify.

Fix: the identifier is a SHA-256 over the circuit id, the model id and the
normalised input commitment. The proof bytes are deliberately excluded so that
re-submitting the same statement cannot mint a second identity.

## Test strategy

The tests generate real Groth16 parameters and proofs with `ark-groth16`
(`generate_random_parameters_with_reduction` / `create_random_proof_with_reduction`)
over a one-public-input circuit. No verifier path is mocked, and no proof bytes
are hand-written, so the pairing check actually executes.

The regression value was confirmed by disabling each guard in turn: with the
guards removed, exactly the four security tests fail and the rest still pass.
That distinguishes "the test asserts an error" from "the test asserts the error
this guard produces".

## What changed

| File | Change |
| --- | --- |
| `src/verification/zkcp.rs` | `verify_groth16_statement(expected_circuit_id, payload)`; commitment-binding check; canonicality round-trip; `ZKML_CIRCUIT_ID`; `NonCanonicalPublicInput` and `CommitmentNotBound` errors |
| `src/api/zkml.rs` | verifies under `ZKML_CIRCUIT_ID`; statement-derived `attestation_id`; real-proof endpoint tests |
| `src/verification/mod.rs` | re-exports |

## Remaining work (not done here)

- **Caller-supplied verifying key.** `verify_groth16_statement` still
  deserializes `payload.verifying_key_bytes`. For ZKML the handler selects the
  key from configuration by `model_id`, which is correct, but the shared
  verifier will accept any structurally valid key. ZKCP's `/v1/verify/zkcp`
  route has the same exposure (tracked as alert #40/#51 in issue #588).
- **`model_id` is not bound into the public inputs.** It selects the verifying
  key, so key choice is bound, but the id itself is only reflected in the
  attestation hash. Binding it into the circuit would require it in the
  statement.
- **No circuit exists yet.** These fixes make the verification path sound with
  respect to its inputs; they do not provide an inference circuit, a trusted
  setup, or a proving key. Until a real circuit and trusted setup exist, the
  endpoint verifies proofs over whatever circuit the configured key belongs to.
- **Unauthenticated route.** `/v1/zkml` is mounted without auth middleware
  (alert #66 in issue #588).
