# BitVM Groth16 state-transition contract

Status: **phase-2 runtime/API/audit boundary; production height source pending**

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

The optional `NEXUS_BITVM_GROTH16_TRUSTED_REGISTRY_JSON` environment value is
a strict JSON object with `expected_bitcoin_network` and a nonempty `records`
array. Each record contains schema version, curve, circuit ID, lowercase VK ID,
public-input count, `nexus-state-transition-v1` layout, enabled flag, and the
base64 canonical Arkworks VK bytes. Unknown fields fail startup.

Each entry records the exact public-input count, the Nexus profile-v1 layout,
and an enabled flag. Registration:

1. recomputes Gateway's domain-separated ID from the exact canonical key
   bytes;
2. rejects an ID mismatch;
3. strictly deserializes a BN254 Arkworks verification key and rejects trailing
   bytes; and
4. caches the parsed/prepared key.

Verification rechecks the stored byte digest and rejects disabled, missing, or
mismatched schema/curve/circuit associations before pairings. A VK ID may map
to exactly one schema/curve/circuit/layout/enablement association; conflicting
duplicates fail closed. Missing configuration never loads a fixture key and
leaves the canonical verifier typed unavailable.

## Runtime service and audit

`src/executor/canonical_bitvm.rs` composes the pure verifier with:

- one configured Bitcoin network;
- an injectable trusted current-height provider; and
- an async immutable receipt store.

The operation order is network check, trusted-height lookup, cryptographic
verification, receipt derivation, immutable persistence, then success. Audit
write/read failures prevent success. Receipt IDs are domain-separated hashes of
the statement hash and canonical proof digest, so exact retries are idempotent
while a different proof for the same statement can produce a distinct record.

Canonical receipts are stored in `canonical_bitvm_receipts`, not the legacy
table containing synthetic confidence/step metadata. The record includes the
profile identifiers, VK ID, statement/proof hashes, roots, witness commitment,
Bitcoin context, backend identity/version, and verification timestamp.

## HTTP API

`POST /v1/bitvm2/verify-state-transition` accepts only named previous/next
roots and the exact Gateway envelope. The route has a 16 KiB body limit and the
outer request rejects unknown fields. Raw VKs and witness data are forbidden.
Success exposes only immutable receipt/profile identifiers and `verified`
status. The stable error policy is:

- `400`: malformed payload or encoding;
- `409`: immutable audit conflict;
- `413`: body too large;
- `422`: invalid statement/context/key/proof;
- `501`: canonical registry absent;
- `503`: trusted height or audit unavailable; and
- `500`: unexpected verifier integrity/backend failure.

The legacy `POST /v1/bitvm2/verify-state-root` route always returns typed `501`
and never deserializes or invokes the removed caller-keyed BLS verifier.

## Ownership

- **Gateway** owns the shared schema-v1 envelope, canonical encoding/hashing,
  parsing contract, and orchestration handoff.
- **Nexus** owns this 12-input state-transition profile, the Arkworks BN254
  backend, trusted key registry, key lifecycle, audit persistence, and API
  migration.
- **Core** owns shared types and invariants. The currently pinned Core helper
  remains non-canonical for this path and is not used by the service or API.
  The Core revision stays pinned in this phase because no approved shared type
  replacement is available.

## Production-readiness gate

This boundary uses real `Groth16::<Bn254>` verification, but the checked-in
test circuit only constrains each public input equal to a witness copy. It is a
deterministic boundary fixture, **not** a production transition circuit.

Production enablement remains blocked until all of the following are reviewed
and approved:

1. a production circuit that enforces the state-transition and witness
   commitment semantics;
2. reproducible circuit/VK artifacts and ceremony/provenance evidence;
3. reviewed key-rotation and operational procedures for the strict registry;
4. an approved trusted Bitcoin-height source wired into production;
5. bounded production observability for registry/height/audit failures; and
6. a shared versioned Gateway/Nexus conformance fixture.

Until that gate is complete, no registered fixture key should be treated as a
production authorization primitive.

The deterministic equality circuit used by tests is fixture-only. A shared
cross-repository JSON proof/VK vector remains follow-up work; it was not added
here to avoid treating a large generated fixture as production configuration.
