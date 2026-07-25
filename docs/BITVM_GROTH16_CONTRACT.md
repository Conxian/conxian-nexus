# BitVM Groth16 state-transition contract

Status: **phase-1 verifier boundary; not production-ready**

This document defines the Nexus profile layered on Gateway's Groth16 schema
v1. The implementation is `src/executor/bitvm_groth16.rs`. It is deliberately
separate from the legacy BLS12-381 route in `src/executor/bitvm.rs`; that route
is not canonical for cross-repository verification.

## Shared envelope

Nexus consumes Gateway's exact schema-v1 envelope and canonical hashing rules
from `conxian-gateway/docs/GROTH16_VERIFIER_CONTRACT.md`:

- curve: BN254;
- scalar encoding: exactly 32-byte, big-endian, canonical values strictly
  below the BN254 scalar modulus;
- proof encoding: Arkworks compressed `A || B || C`, exactly 128 bytes;
- statement and verification-key IDs: Gateway's domain-separated SHA-256
  encodings, including their big-endian `u32` length frames;
- ordered public inputs; no sorting or modular reduction;
- named witness commitment and Bitcoin block context in the statement hash;
- no raw witness material and no unknown JSON fields.

Proof and verification-key decoders must consume every byte. Trailing bytes
are invalid even if Arkworks can deserialize a valid prefix.

## Nexus state-transition profile v1

The stable circuit ID is:

```text
conxian-nexus-state-transition-bn254-v1
```

The circuit has exactly 12 public inputs in this consensus-critical order:

| Slot | Meaning |
| ---: | --- |
| 0 | previous state root, high 128-bit limb |
| 1 | previous state root, low 128-bit limb |
| 2 | next state root, high 128-bit limb |
| 3 | next state root, low 128-bit limb |
| 4 | network tag: mainnet `1`, testnet `2`, signet `3`, regtest `4` |
| 5 | anchor height as `u64` |
| 6 | anchor block hash, high 128-bit limb |
| 7 | anchor block hash, low 128-bit limb |
| 8 | expiry-present flag: `0` or `1` |
| 9 | maximum valid height, or zero when absent |
| 10 | witness commitment, high 128-bit limb |
| 11 | witness commitment, low 128-bit limb |

Each 32-byte root, block hash, or witness commitment is split at byte 16 in
big-endian order. Each half is left-zero-extended to 32 bytes. The source
32-byte value is never interpreted as one field element and is never reduced.

The adapter derives slots 0–9 from named transition and block-context fields.
It derives slots 10–11 from the named witness commitment. The caller-supplied
ordered vector must match all derived slots before statement hashing or pairing
verification.

## Trusted verification-key registry

Runtime requests contain only `verification_key_id`; they never supply raw key
bytes. Nexus owns a trusted startup/config registry keyed by:

```text
(schema_version, curve, circuit_id, verification_key_id)
```

Each entry records the exact public-input count, the Nexus profile-v1 layout,
and an enabled flag. Registration:

1. recomputes Gateway's domain-separated ID from the exact canonical key
   bytes;
2. rejects an ID mismatch;
3. strictly deserializes a BN254 Arkworks verification key and rejects trailing
   bytes; and
4. caches the parsed/prepared key.

Verification rechecks the stored byte digest and rejects disabled, missing, or
mismatched schema/curve/circuit associations before pairings.

## Ownership

- **Gateway** owns the shared schema-v1 envelope, canonical encoding/hashing,
  parsing contract, and orchestration handoff.
- **Nexus** owns this 12-input state-transition profile, the Arkworks BN254
  backend, trusted key registry, key lifecycle, audit persistence, and API
  migration.
- **Core** owns shared types and invariants. The currently pinned Core helper
  is not the canonical implementation for this path and must not be used to
  verify this profile. This phase does not change the Core pin.

## Production-readiness gate

This boundary uses real `Groth16::<Bn254>` verification, but the checked-in
test circuit only constrains each public input equal to a witness copy. It is a
deterministic boundary fixture, **not** a production transition circuit.

Production enablement remains blocked until all of the following are reviewed
and approved:

1. a production circuit that enforces the state-transition and witness
   commitment semantics;
2. reproducible circuit/VK artifacts and ceremony/provenance evidence;
3. explicit trusted-registry configuration and key-rotation operations;
4. API migration away from caller-supplied VK bytes and the legacy BLS route;
5. fail-closed audit persistence and bounded observability; and
6. cross-repository Gateway/Nexus conformance vectors.

Until that gate is complete, no registered fixture key should be treated as a
production authorization primitive.
