//! [R3a] Ziren (ZKM) v2.0.0 BN254 Groth16 verifier.
//!
//! Mirrors the reference implementation in `ProjectZKM/Ziren`
//! (`crates/verifier/src/groth16`) so that a proof accepted here is accepted by
//! the upstream verifier and by the on-chain Solidity verifier. The wire formats
//! are gnark's, not arkworks': points are little-endian with the compression flag
//! in the *most significant* two bits of the first byte, and a Groth16 proof is
//! prefixed with a 4-byte selector equal to the first four bytes of
//! `sha256(verifying_key)`.
//!
//! Anything malformed is rejected. There is no path here that returns success
//! without a completed pairing check.

use ark_bn254::{Bn254, Fr, G1Affine, G2Affine};
use ark_ec::AffineRepr;
use ark_ff::PrimeField;
use ark_groth16::{prepare_verifying_key, Groth16, Proof, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, Compress, Validate};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Length of the Ziren v2.0.0 Groth16 verifying key, in bytes.
pub const ZIREN_VK_LEN: usize = 428;

/// Length of a Ziren Groth16 proof: 4-byte selector + 256 bytes of points.
pub const ZIREN_PROOF_LEN: usize = 4 + 64 + 128 + 64;

/// Groth16 verifying key for Ziren v2.0.0 (`crates/verifier/bn254-vk/groth16_vk.bin`).
pub const ZIREN_V2_GROTH16_VK: &[u8; ZIREN_VK_LEN] =
    include_bytes!("artifacts/ziren_v2.0.0_groth16_vk.bin");

/// Recursion verifying-key root, the third public input.
pub const ZIREN_V2_VK_ROOT: &[u8; 32] = include_bytes!("artifacts/ziren_v2.0.0_vk_root.bin");

const GNARK_MASK: u8 = 0b11 << 6;
const GNARK_COMPRESSED_POSITIVE: u8 = 0b10 << 6;
const GNARK_COMPRESSED_NEGATIVE: u8 = 0b11 << 6;
const GNARK_COMPRESSED_INFINITY: u8 = 0b01 << 6;

const ARK_MASK: u8 = 0b11 << 6;
const ARK_COMPRESSED_POSITIVE: u8 = 0b00 << 6;
const ARK_COMPRESSED_NEGATIVE: u8 = 0b10 << 6;
const ARK_COMPRESSED_INFINITY: u8 = 0b01 << 6;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ZkmError {
    #[error("proof must be {ZIREN_PROOF_LEN} bytes, got {0}")]
    InvalidProofLength(usize),
    #[error("verifying key must be {ZIREN_VK_LEN} bytes, got {0}")]
    InvalidVkLength(usize),
    #[error("public values digest must be 32 bytes, got {0}")]
    InvalidPublicValuesDigestLength(usize),
    #[error("vkey hash must be 32 bytes, got {0}")]
    InvalidVkeyHashLength(usize),
    #[error("proof selector does not match the verifying key")]
    SelectorMismatch,
    #[error("invalid G1 point encoding")]
    G1Compression,
    #[error("invalid G2 point encoding")]
    G2Compression,
    #[error("invalid compressed-point flag byte 0x{0:02x}")]
    InvalidPointFlag(u8),
    #[error("verifying key is truncated or has an unsupported shape")]
    MalformedVerifyingKey,
    #[error("public input is not a valid scalar")]
    InvalidScalar,
    #[error("Groth16 pairing check failed")]
    ProofRejected,
}

/// Verify a Ziren v2.0.0 Groth16 proof.
///
/// * `proof` — the full wire proof, including the 4-byte selector.
/// * `public_values` — the guest's committed public values, as hashed upstream
///   (`sha256` over the raw bytes, then the top three bits cleared).
/// * `vkey_hash` — the program's vkey hash (`vk.bytes32()`), 32 raw bytes.
pub fn verify_ziren_groth16(
    proof: &[u8],
    public_values: &[u8],
    vkey_hash: &[u8],
    vk: &[u8],
) -> Result<(), ZkmError> {
    if proof.len() != ZIREN_PROOF_LEN {
        return Err(ZkmError::InvalidProofLength(proof.len()));
    }
    if vk.len() != ZIREN_VK_LEN {
        return Err(ZkmError::InvalidVkLength(vk.len()));
    }
    if vkey_hash.len() != 32 {
        return Err(ZkmError::InvalidVkeyHashLength(vkey_hash.len()));
    }

    // The selector binds the proof to this exact verifying key.
    let expected: [u8; 4] = Sha256::digest(vk)[..4]
        .try_into()
        .map_err(|_| ZkmError::MalformedVerifyingKey)?;
    if proof[..4] != expected {
        return Err(ZkmError::SelectorMismatch);
    }

    let parsed_vk = parse_verifying_key(vk)?;
    let parsed_proof = parse_proof(&proof[4..])?;

    let public_inputs = [
        Fr::from_be_bytes_mod_order(vkey_hash),
        Fr::from_be_bytes_mod_order(&hash_public_values(public_values)),
        Fr::from_be_bytes_mod_order(ZIREN_V2_VK_ROOT.as_slice()),
    ];

    let prepared = prepare_verifying_key(&parsed_vk);
    let valid = Groth16::<Bn254>::verify_proof(&prepared, &parsed_proof, &public_inputs)
        .map_err(|_| ZkmError::InvalidScalar)?;

    if valid {
        Ok(())
    } else {
        Err(ZkmError::ProofRejected)
    }
}

/// Hash committed public values the way Ziren does: `sha256` with the top three
/// bits of the first byte cleared, so the result is a canonical BN254 scalar.
pub fn hash_public_values(public_values: &[u8]) -> [u8; 32] {
    let mut digest: [u8; 32] = Sha256::digest(public_values).into();
    digest[0] &= 0x1F;
    digest
}

fn parse_verifying_key(vk: &[u8]) -> Result<VerifyingKey<Bn254>, ZkmError> {
    let alpha_g1 = decompress_g1(take::<32>(vk, 0)?)?;
    let beta_g2 = decompress_g2(take::<64>(vk, 64)?)?;
    let gamma_g2 = decompress_g2(take::<64>(vk, 128)?)?;
    let delta_g2 = decompress_g2(take::<64>(vk, 224)?)?;

    let num_k = u32::from_be_bytes(*take::<4>(vk, 288)?) as usize;
    let mut gamma_abc = Vec::with_capacity(num_k);
    let mut offset = 292usize;
    for _ in 0..num_k {
        gamma_abc.push(decompress_g1(take::<32>(vk, offset)?)?);
        offset = offset
            .checked_add(32)
            .ok_or(ZkmError::MalformedVerifyingKey)?;
    }

    // A commitment section may follow; it carries no public-input weight here,
    // but the declared count must fit inside the key or the key is malformed.
    let commitments = u32::from_be_bytes(*take::<4>(vk, offset)?) as usize;
    offset = offset
        .checked_add(4)
        .ok_or(ZkmError::MalformedVerifyingKey)?;
    for _ in 0..commitments {
        let n = u32::from_be_bytes(*take::<4>(vk, offset)?) as usize;
        offset = offset
            .checked_add(4)
            .and_then(|o| o.checked_add(4usize.checked_mul(n)?))
            .ok_or(ZkmError::MalformedVerifyingKey)?;
        if offset > vk.len() {
            return Err(ZkmError::MalformedVerifyingKey);
        }
    }

    Ok(VerifyingKey {
        alpha_g1,
        beta_g2,
        gamma_g2,
        delta_g2,
        gamma_abc_g1: gamma_abc,
    })
}

fn parse_proof(body: &[u8]) -> Result<Proof<Bn254>, ZkmError> {
    Ok(Proof::<Bn254> {
        a: decompressed_g1(take::<64>(body, 0)?)?,
        b: decompressed_g2(take::<128>(body, 64)?)?,
        c: decompressed_g1(take::<64>(body, 192)?)?,
    })
}

fn take<const N: usize>(buf: &[u8], off: usize) -> Result<&[u8; N], ZkmError> {
    let end = off.checked_add(N).ok_or(ZkmError::MalformedVerifyingKey)?;
    buf.get(off..end)
        .and_then(|s| s.try_into().ok())
        .ok_or(ZkmError::MalformedVerifyingKey)
}

/// Reverse every `CHUNK_SIZE`-byte chunk, converting between gnark's
/// little-endian limbs and arkworks' big-endian ones.
fn convert_endianness<const CHUNK_SIZE: usize, const ARRAY_SIZE: usize>(
    bytes: &[u8; ARRAY_SIZE],
) -> [u8; ARRAY_SIZE] {
    let mut out = [0u8; ARRAY_SIZE];
    let (chunks, _) = bytes.as_chunks::<CHUNK_SIZE>();
    for (i, chunk) in chunks.iter().enumerate() {
        for (j, b) in chunk.iter().rev().enumerate() {
            out[i * CHUNK_SIZE + j] = *b;
        }
    }
    out
}

fn gnark_flag_to_ark_flag(msb: u8) -> Result<u8, ZkmError> {
    let ark_flag = match msb & GNARK_MASK {
        GNARK_COMPRESSED_POSITIVE => ARK_COMPRESSED_POSITIVE,
        GNARK_COMPRESSED_NEGATIVE => ARK_COMPRESSED_NEGATIVE,
        GNARK_COMPRESSED_INFINITY => ARK_COMPRESSED_INFINITY,
        other => return Err(ZkmError::InvalidPointFlag(other)),
    };
    Ok((msb & !ARK_MASK) | ark_flag)
}

fn gnark_compressed_to_ark_compressed(x: &[u8]) -> Result<Vec<u8>, ZkmError> {
    let mut out = x.to_vec();
    out[0] = gnark_flag_to_ark_flag(out[0])?;
    out.reverse();
    Ok(out)
}

fn decompress_g1(bytes: &[u8; 32]) -> Result<G1Affine, ZkmError> {
    let converted = gnark_compressed_to_ark_compressed(bytes)?;
    let converted: [u8; 32] = converted.try_into().map_err(|_| ZkmError::G1Compression)?;
    let normalised = convert_endianness::<32, 32>(&convert_endianness::<32, 32>(&converted));
    G1Affine::deserialize_with_mode(normalised.as_slice(), Compress::Yes, Validate::No)
        .map_err(|_| ZkmError::G1Compression)
}

fn decompress_g2(bytes: &[u8; 64]) -> Result<G2Affine, ZkmError> {
    let converted = gnark_compressed_to_ark_compressed(bytes)?;
    let converted: [u8; 64] = converted.try_into().map_err(|_| ZkmError::G2Compression)?;
    let normalised = convert_endianness::<64, 64>(&convert_endianness::<64, 64>(&converted));
    G2Affine::deserialize_with_mode(normalised.as_slice(), Compress::Yes, Validate::No)
        .map_err(|_| ZkmError::G2Compression)
}

fn decompressed_g1(bytes: &[u8; 64]) -> Result<G1Affine, ZkmError> {
    let buf = convert_endianness::<32, 64>(bytes);
    if buf == [0u8; 64] {
        return Ok(G1Affine::zero());
    }
    let with_flag = [&buf[..], &[0u8][..]].concat();
    G1Affine::deserialize_with_mode(with_flag.as_slice(), Compress::No, Validate::Yes)
        .map_err(|_| ZkmError::G1Compression)
}

fn decompressed_g2(bytes: &[u8; 128]) -> Result<G2Affine, ZkmError> {
    let buf = convert_endianness::<64, 128>(bytes);
    if buf == [0u8; 128] {
        return Ok(G2Affine::zero());
    }
    let with_flag = [&buf[..], &[0u8][..]].concat();
    G2Affine::deserialize_with_mode(with_flag.as_slice(), Compress::No, Validate::Yes)
        .map_err(|_| ZkmError::G2Compression)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vk_has_expected_length_and_four_ic_points() {
        assert_eq!(ZIREN_V2_GROTH16_VK.len(), ZIREN_VK_LEN);
        let num_k = u32::from_be_bytes(ZIREN_V2_GROTH16_VK[288..292].try_into().unwrap());
        // Three public inputs plus the constant term.
        assert_eq!(num_k, 4);
    }

    #[test]
    fn public_value_hash_clears_top_three_bits() {
        let digest = hash_public_values(b"ziren");
        assert_eq!(digest[0] & 0xE0, 0);
    }

    #[test]
    fn rejects_wrong_proof_length() {
        let err =
            verify_ziren_groth16(&[0u8; 10], b"pv", &[0u8; 32], ZIREN_V2_GROTH16_VK).unwrap_err();
        assert_eq!(err, ZkmError::InvalidProofLength(10));
    }

    #[test]
    fn rejects_wrong_vk_length() {
        let err = verify_ziren_groth16(&[0u8; ZIREN_PROOF_LEN], b"pv", &[0u8; 32], &[0u8; 7])
            .unwrap_err();
        assert_eq!(err, ZkmError::InvalidVkLength(7));
    }

    #[test]
    fn rejects_selector_mismatch() {
        let mut proof = [0u8; ZIREN_PROOF_LEN];
        proof[..4].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        let err = verify_ziren_groth16(&proof, b"pv", &[0u8; 32], ZIREN_V2_GROTH16_VK).unwrap_err();
        assert_eq!(err, ZkmError::SelectorMismatch);
    }

    #[test]
    fn rejects_noncanonical_point_flag() {
        // 0b00 is unassigned in gnark's encoding, so it must be refused rather
        // than silently mapped onto an arkworks flag.
        let err = gnark_flag_to_ark_flag(0b00).unwrap_err();
        assert_eq!(err, ZkmError::InvalidPointFlag(0b00));

        // The three assigned flags all convert.
        for flag in [
            GNARK_COMPRESSED_POSITIVE,
            GNARK_COMPRESSED_NEGATIVE,
            GNARK_COMPRESSED_INFINITY,
        ] {
            assert!(gnark_flag_to_ark_flag(flag).is_ok());
        }
    }
}
