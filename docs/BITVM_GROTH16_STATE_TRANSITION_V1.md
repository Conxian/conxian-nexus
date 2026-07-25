# BitVM Groth16 State-Transition Profile v1

Status: implemented for `CON-1533` as a narrow Nexus verification profile.

## Ownership

- **Gateway owns envelope and policy contract evolution**, including the
  canonical domains, framing, statement layout, key identity rules, and future
  schema versions.
- **Nexus owns canonical pre/post state materialization plus local pairing
  execution and required audit persistence** for
  `conxian-nexus-bitvm-state-transition-v1`.
- **Core is not currently the verifier owner or import path** for this profile.
  Nexus removed the historical Core Groth16 verifier from this profile and does
  not use Core's modulo-reducing helper here. The repository-level Core
  dependency remains required by separate identity, wallet, contract-bridge,
  service-status, Kwil-signing, and DLC-signing callers; it is not part of this
  profile's verification path.
- Circuit and verification-key approval/governance is separate from this byte
  contract. A key being syntactically registrable does not approve its circuit.

## Fixed profile

- `schema_version = 1`
- `curve = "bn254"`
- `circuit_id = "conxian-nexus-bitvm-state-transition-v1"`
- Proof and verification-key bytes use Arkworks 0.6 canonical compressed
  encoding. A proof is exactly 128 bytes (`A || B || C`) with no trailing bytes.
- Requests contain a verification-key ID, never verification-key bytes.
- Registry records are deployment-controlled, startup-validated, disabled by
  default, and associated with the exact schema, curve, circuit, and derived
  key ID. No usable key causes a typed `503`; there is no fallback key.

The seven ordered 32-byte big-endian BN254 scalar encodings are:

0. previous root, high 128 bits, left-zero-padded
1. previous root, low 128 bits, left-zero-padded
2. next root, high 128 bits, left-zero-padded
3. next root, low 128 bits, left-zero-padded
4. `steps_verified` as `u64`, left-zero-padded
5. witness commitment, high 128 bits, left-zero-padded
6. witness commitment, low 128 bits, left-zero-padded

Every value must be strictly below the BN254 scalar modulus. Nexus reconstructs
the vector from named fields and byte-compares it before invoking Arkworks. No
modulo reduction is allowed.

Roots are exactly `0x` plus 64 lowercase hexadecimal characters. The witness
commitment is exactly 64 lowercase hexadecimal characters without `0x`.

## Canonical statement and hashes

Nexus reuses Gateway v1 byte framing exactly:

```text
SHA256(domain || u32_be(payload_length) || payload)
```

Domains:

- `CONXIAN-GROTH16-STATEMENT-ENCODING-V1`
- `CONXIAN-GROTH16-STATEMENT-HASH-V1`
- `CONXIAN-GROTH16-VERIFICATION-KEY-ID-V1`
- `CONXIAN-GROTH16-PROOF-DIGEST-V1`

The statement contains the schema, curve/field tags, circuit ID, verification
key ID, seven ordered public inputs, witness commitment, and Bitcoin network,
anchor height, anchor hash, and optional expiry height.

## Audit and response

`statement_hash` is the immutable/idempotent audit identity. Exact replays are
accepted idempotently; a replay whose proof digest or other immutable field
differs fails closed. `trace_id` is optional correlation metadata only and is
not part of replay identity. A successful response is emitted only after
`bitvm_groth16_v1_audit` persistence succeeds. The legacy confidence table
remains untouched and receives no new success writes from this profile.

Success contains only `valid`, `statement_hash`, `circuit_id`,
`verification_key_id`, and circuit-authenticated `steps_verified`.

## Explicit limitations

This profile does **not** claim or establish:

- complete BitVM execution or protocol correctness;
- Bitcoin transaction inclusion, SPV proof, confirmations, or finality;
- challenge/dispute execution or economic security;
- circuit or verification-key governance approval;
- production or mainnet readiness.

The Bitcoin block context is structurally validated and authenticated by the
statement. Nexus currently has no trusted live Bitcoin height/finality provider
in this path, so the supplied anchor and optional expiry are bound but finality
is not established.

The checked-in fixture key under `tests/fixtures/` is unmistakably test-only,
generated deterministically by `examples/generate_bitvm_groth16_fixture.rs`,
and is never enabled by production defaults.
