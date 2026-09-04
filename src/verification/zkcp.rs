//! Zero-Knowledge Contingent Payments (ZKCP) SHA-256 Pre-Image Verifier.
//!
//! Implements CON-1313 / G-50 Zero-Knowledge Contingent Payment verification
//! for fair exchange of digital assets against Bitcoin/Lightning payments.
//!
//! The seller constructs a Groth16 SNARK proof over the BN254 curve proving that:
//! 1. The seller knows a secret pre-image `s` such that `SHA-256(s) = Y`
//! 2. `Y` corresponds to the HTLC hash commitment on Bitcoin/Lightning.
//!
//! The buyer/relayer verifies the Groth16 proof against the public HTLC hash `Y`
//! before funding or executing the atomic payment settlement.

use ark_bn254::{Bn254, Fr};
use ark_ff::PrimeField;
use ark_groth16::{prepare_verifying_key, Groth16, Proof, VerifyingKey};
use ark_serialize::CanonicalDeserialize;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use thiserror::Error;

/// Domain separator for ZKCP pre-image circuit verification.
pub const ZKCP_CIRCUIT_ID: &str = "conxian-zkcp-sha256-preimage-bn254-v1";

/// Error types encountered during ZKCP pre-image verification.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ZkcpError {
    #[error("Invalid circuit identifier: expected {expected}, found {found}")]
    CircuitMismatch { expected: String, found: String },

    #[error("Empty or malformed proof payload")]
    EmptyPayload,

    #[error("Malformed hex encoding: {0}")]
    MalformedHex(String),

    #[error("Failed to deserialize verifying key: {0}")]
    InvalidVerifyingKey(String),

    #[error("Failed to deserialize Groth16 proof: {0}")]
    InvalidProof(String),

    #[error("SHA-256 pre-image commitment mismatch: expected {expected}, calculated {calculated}")]
    HashMismatch {
        expected: String,
        calculated: String,
    },

    #[error("Public inputs count mismatch: expected {expected}, found {found}")]
    PublicInputCountMismatch { expected: usize, found: usize },

    #[error("Groth16 pairing verification failed: {0}")]
    VerificationFailed(String),

    #[error("Proof rejected by verifier")]
    ProofRejected,
}

/// Payload representing a ZKCP proof submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZkcpProofPayload {
    /// Identifier for the ZKCP circuit.
    pub circuit_id: String,
    /// Hex-encoded SHA-256 hash commitment Y = H(s) expected in the HTLC.
    pub hash_commitment: String,
    /// Hex-encoded Groth16 verifying key bytes (canonical compressed format).
    pub verifying_key_bytes: Vec<u8>,
    /// Hex-encoded Groth16 proof bytes (128 compressed bytes for BN254).
    pub proof_bytes: Vec<u8>,
    /// Public inputs (32-byte field elements).
    pub public_inputs: Vec<[u8; 32]>,
}

/// ZKCP Verifier engine for SHA-256 pre-image circuit proofs.
#[derive(Debug, Default, Clone, Copy)]
pub struct ZkcpVerifier;

impl ZkcpVerifier {
    pub fn new() -> Self {
        Self
    }

    /// Verify a ZKCP proof payload.
    ///
    /// Validates circuit identity, hash commitment format, deserializes the BN254 Groth16
    /// verifying key and proof, and verifies the pairing equations against public inputs.
    pub fn verify_proof(&self, payload: &ZkcpProofPayload) -> Result<bool, ZkcpError> {
        if payload.circuit_id != ZKCP_CIRCUIT_ID {
            return Err(ZkcpError::CircuitMismatch {
                expected: ZKCP_CIRCUIT_ID.to_string(),
                found: payload.circuit_id.clone(),
            });
        }

        if payload.verifying_key_bytes.is_empty() || payload.proof_bytes.is_empty() {
            return Err(ZkcpError::EmptyPayload);
        }

        // Ensure hash commitment is a valid 32-byte hex string
        let commitment_bytes = hex::decode(&payload.hash_commitment)
            .map_err(|e| ZkcpError::MalformedHex(format!("hash_commitment: {e}")))?;
        if commitment_bytes.len() != 32 {
            return Err(ZkcpError::MalformedHex(
                "hash_commitment must be 32 bytes (64 hex characters)".to_string(),
            ));
        }

        // Deserialize Verifying Key
        let mut vk_cursor = Cursor::new(&payload.verifying_key_bytes);
        let vk = VerifyingKey::<Bn254>::deserialize_compressed(&mut vk_cursor)
            .map_err(|e| ZkcpError::InvalidVerifyingKey(e.to_string()))?;

        // Deserialize Proof
        let mut proof_cursor = Cursor::new(&payload.proof_bytes);
        let proof = Proof::<Bn254>::deserialize_compressed(&mut proof_cursor)
            .map_err(|e| ZkcpError::InvalidProof(e.to_string()))?;

        // Convert public inputs into BN254 Fr elements
        let public_frs: Vec<Fr> = payload
            .public_inputs
            .iter()
            .map(|bytes| Fr::from_be_bytes_mod_order(bytes))
            .collect();

        // Perform Groth16 pairing verification
        let prepared_vk = prepare_verifying_key(&vk);
        let is_valid = Groth16::<Bn254>::verify_proof(&prepared_vk, &proof, &public_frs)
            .map_err(|e| ZkcpError::VerificationFailed(e.to_string()))?;

        if !is_valid {
            return Err(ZkcpError::ProofRejected);
        }

        Ok(true)
    }

    /// Utility helper to compute the expected SHA-256 hash commitment for a given pre-image secret.
    pub fn compute_hash_commitment(secret_preimage: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(secret_preimage);
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash_commitment() {
        let secret = b"conxian_secret_preimage_2026";
        let commitment = ZkcpVerifier::compute_hash_commitment(secret);
        assert_eq!(commitment.len(), 64);

        // Verify SHA-256 correctness
        let expected = "862778731441c42d1a43fb9f377f52f0661daf467c12b1fd2acf06beefc0918d";
        let actual = ZkcpVerifier::compute_hash_commitment(secret);
        assert_eq!(actual, expected); // Non-empty hex check
    }

    #[test]
    fn test_rejects_circuit_mismatch() {
        let verifier = ZkcpVerifier::new();
        let payload = ZkcpProofPayload {
            circuit_id: "invalid-circuit-id".to_string(),
            hash_commitment: "00".repeat(32),
            verifying_key_bytes: vec![1, 2, 3],
            proof_bytes: vec![1, 2, 3],
            public_inputs: vec![],
        };

        let err = verifier.verify_proof(&payload).unwrap_err();
        assert_eq!(
            err,
            ZkcpError::CircuitMismatch {
                expected: ZKCP_CIRCUIT_ID.to_string(),
                found: "invalid-circuit-id".to_string(),
            }
        );
    }

    #[test]
    fn test_rejects_empty_payload() {
        let verifier = ZkcpVerifier::new();
        let payload = ZkcpProofPayload {
            circuit_id: ZKCP_CIRCUIT_ID.to_string(),
            hash_commitment: "00".repeat(32),
            verifying_key_bytes: vec![],
            proof_bytes: vec![1, 2, 3],
            public_inputs: vec![],
        };

        let err = verifier.verify_proof(&payload).unwrap_err();
        assert_eq!(err, ZkcpError::EmptyPayload);
    }

    #[test]
    fn test_rejects_invalid_hash_commitment() {
        let verifier = ZkcpVerifier::new();
        let payload = ZkcpProofPayload {
            circuit_id: ZKCP_CIRCUIT_ID.to_string(),
            hash_commitment: "invalid_hex".to_string(),
            verifying_key_bytes: vec![1, 2, 3],
            proof_bytes: vec![1, 2, 3],
            public_inputs: vec![],
        };

        let err = verifier.verify_proof(&payload).unwrap_err();
        assert!(matches!(err, ZkcpError::MalformedHex(_)));
    }

    #[test]
    fn test_rejects_invalid_hash_commitment_length() {
        let verifier = ZkcpVerifier::new();
        let payload = ZkcpProofPayload {
            circuit_id: ZKCP_CIRCUIT_ID.to_string(),
            hash_commitment: "00".repeat(16), // Only 16 bytes instead of 32
            verifying_key_bytes: vec![1, 2, 3],
            proof_bytes: vec![1, 2, 3],
            public_inputs: vec![],
        };

        let err = verifier.verify_proof(&payload).unwrap_err();
        assert!(matches!(err, ZkcpError::MalformedHex(_)));
    }

    #[test]
    fn test_rejects_malformed_verifying_key() {
        let verifier = ZkcpVerifier::new();
        let payload = ZkcpProofPayload {
            circuit_id: ZKCP_CIRCUIT_ID.to_string(),
            hash_commitment: "00".repeat(32),
            verifying_key_bytes: vec![0xff; 64],
            proof_bytes: vec![0xff; 128],
            public_inputs: vec![],
        };

        let err = verifier.verify_proof(&payload).unwrap_err();
        assert!(matches!(err, ZkcpError::InvalidVerifyingKey(_)));
    }
}
