//! Canonical Proof Envelope & Verifier Ownership Contract Alignment (Candidate C).
//!
//! Standardized proof surface envelope schema establishing proof-system metadata,
//! curve parameters, verifying key digest commitments, public input bindings,
//! state root bindings, and verifier ownership assertions across Nexus and Gateway boundaries.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const PROOF_ENVELOPE_VERIFIER_ID: &str = "NEXUS-PROOF-ENVELOPE-V1";

#[derive(Debug, Error)]
pub enum ProofEnvelopeError {
    #[error("Missing envelope payload field: {0}")]
    MissingField(String),
    #[error("Unsupported curve: {0}")]
    UnsupportedCurve(String),
    #[error("Unsupported proof system: {0}")]
    UnsupportedProofSystem(String),
    #[error("Verifying key hash mismatch: expected {expected}, computed {computed}")]
    VkHashMismatch { expected: String, computed: String },
    #[error("Public inputs hash mismatch: expected {expected}, computed {computed}")]
    PublicInputsHashMismatch { expected: String, computed: String },
    #[error("Empty or invalid proof payload")]
    InvalidProof,
    #[error("State root mismatch: expected {expected}, computed {computed}")]
    StateRootMismatch { expected: String, computed: String },
    #[error("Verifier ownership assertion failed for verifier '{verifier}': {reason}")]
    OwnershipError { verifier: String, reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProofSystem {
    #[serde(rename = "groth16_bn254")]
    Groth16Bn254,
    #[serde(rename = "groth16_bls12_381")]
    Groth16Bls12381,
    #[serde(rename = "bip340_schnorr")]
    Bip340Schnorr,
    #[serde(rename = "garbled_circuit_bitvm3")]
    GarbledCircuitBitvm3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofEnvelopePayload {
    pub envelope_id: String,
    pub proof_system: ProofSystem,
    pub curve: String,
    pub verifying_key_b64: String,
    pub expected_vk_hash: String,
    pub proof_bytes_b64: String,
    pub public_inputs: Vec<String>,
    pub expected_public_inputs_hash: String,
    pub state_root_hex: Option<String>,
    pub verifier_owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofEnvelopeResponse {
    pub envelope_id: String,
    pub valid: bool,
    pub verifier_id: &'static str,
    pub proof_system: ProofSystem,
    pub curve: String,
    pub computed_vk_hash: String,
    pub computed_public_inputs_hash: String,
    pub state_root_bound: bool,
    pub verifier_owner_verified: bool,
    pub message: String,
}

pub struct ProofEnvelopeVerifier;

impl ProofEnvelopeVerifier {
    pub fn verify_envelope(
        payload: &ProofEnvelopePayload,
    ) -> Result<ProofEnvelopeResponse, ProofEnvelopeError> {
        if payload.envelope_id.trim().is_empty() {
            return Err(ProofEnvelopeError::MissingField("envelope_id".into()));
        }
        if payload.verifying_key_b64.trim().is_empty() {
            return Err(ProofEnvelopeError::MissingField("verifying_key_b64".into()));
        }
        if payload.proof_bytes_b64.trim().is_empty() {
            return Err(ProofEnvelopeError::MissingField("proof_bytes_b64".into()));
        }
        if payload.verifier_owner.trim().is_empty() {
            return Err(ProofEnvelopeError::MissingField("verifier_owner".into()));
        }

        // 1. Verify Curve Alignment
        let curve_clean = payload.curve.trim().to_lowercase();
        match payload.proof_system {
            ProofSystem::Groth16Bn254 => {
                if curve_clean != "bn254" && curve_clean != "alt_bn128" {
                    return Err(ProofEnvelopeError::UnsupportedCurve(payload.curve.clone()));
                }
            }
            ProofSystem::Groth16Bls12381 => {
                if curve_clean != "bls12_381" && curve_clean != "bls12-381" {
                    return Err(ProofEnvelopeError::UnsupportedCurve(payload.curve.clone()));
                }
            }
            ProofSystem::Bip340Schnorr => {
                if curve_clean != "secp256k1" {
                    return Err(ProofEnvelopeError::UnsupportedCurve(payload.curve.clone()));
                }
            }
            ProofSystem::GarbledCircuitBitvm3 => {
                if curve_clean != "bitcoin_script" && curve_clean != "raw" {
                    return Err(ProofEnvelopeError::UnsupportedCurve(payload.curve.clone()));
                }
            }
        }

        // 2. Base64 Decode Verifying Key & Compute SHA-256 Digest
        let vk_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &payload.verifying_key_b64,
        )
        .map_err(|_| ProofEnvelopeError::InvalidProof)?;

        if vk_bytes.is_empty() {
            return Err(ProofEnvelopeError::InvalidProof);
        }

        let computed_vk_hash = hex::encode(Sha256::digest(&vk_bytes));
        if !payload.expected_vk_hash.is_empty()
            && !computed_vk_hash.eq_ignore_ascii_case(&payload.expected_vk_hash)
        {
            return Err(ProofEnvelopeError::VkHashMismatch {
                expected: payload.expected_vk_hash.clone(),
                computed: computed_vk_hash,
            });
        }

        // 3. Compute Public Inputs SHA-256 Digest Commitment
        let mut hasher = Sha256::new();
        for input in &payload.public_inputs {
            hasher.update(input.as_bytes());
            hasher.update(b":");
        }
        let computed_inputs_hash = hex::encode(hasher.finalize());

        if !payload.expected_public_inputs_hash.is_empty()
            && !computed_inputs_hash.eq_ignore_ascii_case(&payload.expected_public_inputs_hash)
        {
            return Err(ProofEnvelopeError::PublicInputsHashMismatch {
                expected: payload.expected_public_inputs_hash.clone(),
                computed: computed_inputs_hash,
            });
        }

        // 4. Verify Base64 Proof Bytes
        let proof_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &payload.proof_bytes_b64,
        )
        .map_err(|_| ProofEnvelopeError::InvalidProof)?;

        if proof_bytes.is_empty() {
            return Err(ProofEnvelopeError::InvalidProof);
        }

        // 5. Verify State Root Commitment Binding if present
        let state_root_bound = if let Some(ref root_hex) = payload.state_root_hex {
            let clean_root = root_hex.trim_start_matches("0x");
            clean_root.len() == 64 && hex::decode(clean_root).is_ok()
        } else {
            false
        };

        // 6. Verifier Ownership Validation
        let owner_clean = payload.verifier_owner.trim();
        if !owner_clean.starts_with("conxian") && !owner_clean.starts_with("nexus") {
            return Err(ProofEnvelopeError::OwnershipError {
                verifier: owner_clean.to_string(),
                reason: "Owner prefix must be 'conxian' or 'nexus'".into(),
            });
        }

        Ok(ProofEnvelopeResponse {
            envelope_id: payload.envelope_id.clone(),
            valid: true,
            verifier_id: PROOF_ENVELOPE_VERIFIER_ID,
            proof_system: payload.proof_system.clone(),
            curve: payload.curve.clone(),
            computed_vk_hash,
            computed_public_inputs_hash: computed_inputs_hash,
            state_root_bound,
            verifier_owner_verified: true,
            message: "Proof envelope schema, cryptographic commitments, and verifier ownership verified successfully".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_valid_payload() -> ProofEnvelopePayload {
        let vk_bytes = b"sample_verifying_key_bytes";
        let vk_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, vk_bytes);
        let expected_vk_hash = hex::encode(Sha256::digest(vk_bytes));

        let proof_bytes = b"sample_groth16_proof_bytes";
        let proof_b64 =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, proof_bytes);

        let public_inputs = vec!["0x1234".to_string(), "0x5678".to_string()];
        let mut hasher = Sha256::new();
        for input in &public_inputs {
            hasher.update(input.as_bytes());
            hasher.update(b":");
        }
        let expected_inputs_hash = hex::encode(hasher.finalize());

        ProofEnvelopePayload {
            envelope_id: "env-001".into(),
            proof_system: ProofSystem::Groth16Bn254,
            curve: "bn254".into(),
            verifying_key_b64: vk_b64,
            expected_vk_hash,
            proof_bytes_b64: proof_b64,
            public_inputs,
            expected_public_inputs_hash: expected_inputs_hash,
            state_root_hex: Some(
                "0x11223344556677889900aabbccddeeff11223344556677889900aabbccddeeff".into(),
            ),
            verifier_owner: "conxian:nexus:gateway_bridge".into(),
        }
    }

    #[test]
    fn test_valid_proof_envelope() {
        let payload = sample_valid_payload();
        let res = ProofEnvelopeVerifier::verify_envelope(&payload).unwrap();
        assert!(res.valid);
        assert!(res.state_root_bound);
        assert!(res.verifier_owner_verified);
        assert_eq!(res.verifier_id, PROOF_ENVELOPE_VERIFIER_ID);
    }

    #[test]
    fn test_curve_mismatch_rejected() {
        let mut payload = sample_valid_payload();
        payload.curve = "secp256k1".into();
        let err = ProofEnvelopeVerifier::verify_envelope(&payload).unwrap_err();
        assert!(matches!(err, ProofEnvelopeError::UnsupportedCurve(_)));
    }

    #[test]
    fn test_vk_hash_mismatch_rejected() {
        let mut payload = sample_valid_payload();
        payload.expected_vk_hash = "00".repeat(32);
        let err = ProofEnvelopeVerifier::verify_envelope(&payload).unwrap_err();
        assert!(matches!(err, ProofEnvelopeError::VkHashMismatch { .. }));
    }

    #[test]
    fn test_verifier_owner_rejected() {
        let mut payload = sample_valid_payload();
        payload.verifier_owner = "unauthorized_owner".into();
        let err = ProofEnvelopeVerifier::verify_envelope(&payload).unwrap_err();
        assert!(matches!(err, ProofEnvelopeError::OwnershipError { .. }));
    }
}
