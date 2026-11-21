//! [R3a] Ziren (ZKM) v2.0.0 Groth16 verifier tests.
//!
//! The fixture is the upstream `fibonacci_groth16_proof.json` shipped by
//! `ProjectZKM/ziren-wasm-verifier` at `ziren-v2.0.0`
//! (`5007c535e7969ff3460b42b195f259af98c4e8aa`), with the verifying key and
//! recursion vk root taken from `ProjectZKM/Ziren`
//! (`c9988a098d92bc55a85ca9307c470d1f97be3337`, `crates/verifier/bn254-vk/`).
//! It is public proof material: no proving key is present.

use conxian_nexus::verification::zkm::{
    hash_public_values, verify_ziren_groth16, ZkmError, ZIREN_PROOF_LEN, ZIREN_V2_GROTH16_VK,
    ZIREN_VK_LEN,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
struct Fixture {
    proof: String,
    public_inputs: String,
    vkey_hash: String,
}

fn load_fixture() -> Fixture {
    let raw = include_str!("fixtures/zkm/fibonacci_groth16_proof.json");
    serde_json::from_str(raw).expect("fixture must parse")
}

fn decode(hex_str: &str) -> Vec<u8> {
    let s = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    hex::decode(s).expect("valid hex")
}

#[test]
fn fixture_proof_length_matches_wire_format() {
    let f = load_fixture();
    assert_eq!(decode(&f.proof).len(), ZIREN_PROOF_LEN);
}

#[test]
fn fixture_vkey_hash_is_32_bytes() {
    let f = load_fixture();
    assert_eq!(decode(&f.vkey_hash).len(), 32);
}

#[test]
fn upstream_fibonacci_proof_verifies() {
    let f = load_fixture();
    let proof = decode(&f.proof);
    let public_values = decode(&f.public_inputs);
    let vkey_hash = decode(&f.vkey_hash);

    verify_ziren_groth16(&proof, &public_values, &vkey_hash, ZIREN_V2_GROTH16_VK)
        .expect("the upstream fibonacci proof must verify");
}

#[test]
fn tampered_proof_is_rejected() {
    let f = load_fixture();
    let mut proof = decode(&f.proof);
    // Flip a bit deep inside the proof body, past the selector.
    proof[100] ^= 0x01;

    let err = verify_ziren_groth16(
        &proof,
        &decode(&f.public_inputs),
        &decode(&f.vkey_hash),
        ZIREN_V2_GROTH16_VK,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            ZkmError::ProofRejected | ZkmError::G1Compression | ZkmError::G2Compression
        ),
        "tampered proof must not verify, got {err:?}"
    );
}

#[test]
fn wrong_public_values_are_rejected() {
    let f = load_fixture();
    let mut public_values = decode(&f.public_inputs);
    public_values[0] ^= 0xFF;

    let err = verify_ziren_groth16(
        &decode(&f.proof),
        &public_values,
        &decode(&f.vkey_hash),
        ZIREN_V2_GROTH16_VK,
    )
    .unwrap_err();

    assert_eq!(err, ZkmError::ProofRejected);
}

#[test]
fn wrong_vkey_hash_is_rejected() {
    let f = load_fixture();
    let mut vkey_hash = decode(&f.vkey_hash);
    vkey_hash[1] ^= 0xFF;

    let err = verify_ziren_groth16(
        &decode(&f.proof),
        &decode(&f.public_inputs),
        &vkey_hash,
        ZIREN_V2_GROTH16_VK,
    )
    .unwrap_err();

    assert_eq!(err, ZkmError::ProofRejected);
}

#[test]
fn selector_must_match_verifying_key() {
    let f = load_fixture();
    let mut proof = decode(&f.proof);
    proof[0] ^= 0xFF;

    let err = verify_ziren_groth16(
        &proof,
        &decode(&f.public_inputs),
        &decode(&f.vkey_hash),
        ZIREN_V2_GROTH16_VK,
    )
    .unwrap_err();

    assert_eq!(err, ZkmError::SelectorMismatch);
}

#[test]
fn trailing_bytes_are_rejected() {
    let f = load_fixture();
    let mut proof = decode(&f.proof);
    proof.push(0);

    let err = verify_ziren_groth16(
        &proof,
        &decode(&f.public_inputs),
        &decode(&f.vkey_hash),
        ZIREN_V2_GROTH16_VK,
    )
    .unwrap_err();

    assert!(
        matches!(err, ZkmError::InvalidProofLength(_)),
        "got {err:?}"
    );
}

#[test]
fn verifying_key_selector_is_sha256_prefix() {
    let expected: [u8; 4] = Sha256::digest(ZIREN_V2_GROTH16_VK)[..4].try_into().unwrap();
    let f = load_fixture();
    let proof = decode(&f.proof);
    assert_eq!(&proof[..4], &expected);
}

#[test]
fn public_value_hash_is_stable_and_masked() {
    let a = hash_public_values(b"fibonacci");
    let b = hash_public_values(b"fibonacci");
    assert_eq!(a, b);
    assert_eq!(a[0] & 0xE0, 0);
    assert_eq!(ZIREN_V2_GROTH16_VK.len(), ZIREN_VK_LEN);
}
