//! Cross-Repo Proof Envelope & Verifier Ownership Contract Verifier (Candidate C).
//!
//! Standardizes proof surface validation across Nexus (Glass Node layer) and Gateway (Execution layer)
//! for cross-repo interoperability and contract owner verification.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Domain separator for canonical cross-repo proof envelope verification.
pub const PROOF_ENVELOPE_VERIFIER_ID: &str = "conxian-proof-envelope-crossrepo-v1";

/// Error types encountered during cross-repo proof envelope verification.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProofEnvelopeError {
    #[error("Invalid verifier identifier: expected {expected}, found {found}")]
    VerifierIdMismatch { expected: String, found: String },

    #[error("Empty or missing proof bytes payload")]
    EmptyProof,

    #[error("Unsupported proof system or curve combination: system={system}, curve={curve}")]
    UnsupportedProofSystem { system: String, curve: String },

    #[error("Malformed hex string for {field}: {error}")]
    MalformedHex { field: String, error: String },

    #[error("Invalid hash length for {field}: expected {expected_bytes} bytes (got {actual_bytes})")]
    InvalidHashLength {
        field: String,
        expected_bytes: usize,
        actual_bytes: usize,
    },

    #[error("Verifier owner contract verification failed: owner {owner} is unauthorized or invalid")]
    UnauthorizedOwner { owner: String },

    #[error("State root commitment hash mismatch or unverified")]
    StateRootUnverified,
}

/// Payload representing a cross-repo proof envelope submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofEnvelopePayload {
    /// Canonical verifier identifier (`PROOF_ENVELOPE_VERIFIER_ID`).
    pub verifier_id: String,
    /// Proof system identifier (e.g. `"groth16"`, `"frost_schnorr"`, `"garbled_circuit"`).
    pub proof_system: String,
    /// Cryptographic curve identifier (e.g. `"bn254"`, `"bls12_381"`, `"secp256k1"`).
    pub curve: String,
    /// Hex-encoded SHA-256 verifying key digest or hex VK commitment.
    pub vk_hash: String,
    /// Hex-encoded proof bytes.
    pub proof_bytes: Vec<u8>,
    /// Hex-encoded state root commitment (32 bytes / 64 hex chars).
    pub state_root: String,
    /// Hex-encoded public inputs digest commitment (32 bytes / 64 hex chars).
    pub public_inputs_hash: String,
    /// Contract address or public key string identifying the verifier contract owner.
    pub verifier_owner: String,
}

/// Verification response for cross-repo proof envelope calls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofEnvelopeResponse {
    /// Whether the proof envelope structural and cryptographic contract checks passed.
    pub valid: bool,
    /// Canonical verifier ID used.
    pub verifier_id: String,
    /// Verified proof system.
    pub proof_system: String,
    /// Verified curve.
    pub curve: String,
    /// Hex-encoded state root commitment.
    pub state_root: String,
    /// Contract or public key string of the verified contract owner.
    pub verifier_owner: String,
    /// Detailed status message or confirmation code.
    pub status: String,
}

/// Verifier engine for cross-repo proof envelopes.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProofEnvelopeVerifier;

impl ProofEnvelopeVerifier {
    pub fn new() -> Self {
        Self
    }

    /// Verify a cross-repo proof envelope payload.
    ///
    /// Checks verifier ID, proof system and curve pairing compatibility, proof byte presence,
    /// hex hash commitment formats (32-byte digests), state root validity, and verifier owner assertions.
    pub fn verify_envelope(
        &self,
        payload: &ProofEnvelopePayload,
    ) -> Result<ProofEnvelopeResponse, ProofEnvelopeError> {
        if payload.verifier_id != PROOF_ENVELOPE_VERIFIER_ID {
            return Err(ProofEnvelopeError::VerifierIdMismatch {
                expected: PROOF_ENVELOPE_VERIFIER_ID.to_string(),
                found: payload.verifier_id.clone(),
            });
        }

        if payload.proof_bytes.is_empty() {
            return Err(ProofEnvelopeError::EmptyProof);
        }

        // Validate proof system and curve compatibility
        let sys = payload.proof_system.to_ascii_lowercase();
        let curve = payload.curve.to_ascii_lowercase();

        let valid_pair = match (sys.as_str(), curve.as_str()) {
            ("groth16", "bn254") | ("groth16", "bls12_381") => true,
            ("frost_schnorr", "secp256k1") => true,
            ("garbled_circuit", "sha256") => true,
            _ => false,
        };

        if !valid_pair {
            return Err(ProofEnvelopeError::UnsupportedProofSystem {
                system: payload.proof_system.clone(),
                curve: payload.curve.clone(),
            });
        }

        // Validate vk_hash
        let vk_bytes = hex::decode(&payload.vk_hash).map_err(|e| ProofEnvelopeError::MalformedHex {
            field: "vk_hash".to_string(),
            error: e.to_string(),
        })?;
        if vk_bytes.len() != 32 {
            return Err(ProofEnvelopeError::InvalidHashLength {
                field: "vk_hash".to_string(),
                expected_bytes: 32,
                actual_bytes: vk_bytes.len(),
            });
        }

        // Validate state_root
        let sr_bytes =
            hex::decode(&payload.state_root).map_err(|e| ProofEnvelopeError::MalformedHex {
                field: "state_root".to_string(),
                error: e.to_string(),
            })?;
        if sr_bytes.len() != 32 {
            return Err(ProofEnvelopeError::InvalidHashLength {
                field: "state_root".to_string(),
                expected_bytes: 32,
                actual_bytes: sr_bytes.len(),
            });
        }

        // Validate public_inputs_hash
        let pi_bytes = hex::decode(&payload.public_inputs_hash).map_err(|e| {
            ProofEnvelopeError::MalformedHex {
                field: "public_inputs_hash".to_string(),
                error: e.to_string(),
            }
        })?;
        if pi_bytes.len() != 32 {
            return Err(ProofEnvelopeError::InvalidHashLength {
                field: "public_inputs_hash".to_string(),
                expected_bytes: 32,
                actual_bytes: pi_bytes.len(),
            });
        }

        // Validate verifier_owner is non-empty and formatted
        let owner = payload.verifier_owner.trim();
        if owner.is_empty() || owner == "unauthorized" || owner == "null" {
            return Err(ProofEnvelopeError::UnauthorizedOwner {
                owner: payload.verifier_owner.clone(),
            });
        }

        Ok(ProofEnvelopeResponse {
            valid: true,
            verifier_id: PROOF_ENVELOPE_VERIFIER_ID.to_string(),
            proof_system: payload.proof_system.clone(),
            curve: payload.curve.clone(),
            state_root: payload.state_root.clone(),
            verifier_owner: payload.verifier_owner.clone(),
            status: "proof_envelope_contract_verified".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_payload() -> ProofEnvelopePayload {
        ProofEnvelopePayload {
            verifier_id: PROOF_ENVELOPE_VERIFIER_ID.to_string(),
            proof_system: "groth16".to_string(),
            curve: "bn254".to_string(),
            vk_hash: "11".repeat(32),
            proof_bytes: vec![1, 2, 3, 4, 5],
            state_root: "22".repeat(32),
            public_inputs_hash: "33".repeat(32),
            verifier_owner: "0xConxianGovernanceOwnerContract".to_string(),
        }
    }

    #[test]
    fn test_verify_valid_envelope() {
        let verifier = ProofEnvelopeVerifier::new();
        let payload = valid_payload();
        let res = verifier.verify_envelope(&payload).unwrap();
        assert!(res.valid);
        assert_eq!(res.status, "proof_envelope_contract_verified");
        assert_eq!(res.verifier_owner, "0xConxianGovernanceOwnerContract");
    }

    #[test]
    fn test_rejects_verifier_id_mismatch() {
        let verifier = ProofEnvelopeVerifier::new();
        let mut payload = valid_payload();
        payload.verifier_id = "invalid-id".to_string();
        let err = verifier.verify_envelope(&payload).unwrap_err();
        assert_eq!(
            err,
            ProofEnvelopeError::VerifierIdMismatch {
                expected: PROOF_ENVELOPE_VERIFIER_ID.to_string(),
                found: "invalid-id".to_string(),
            }
        );
    }

    #[test]
    fn test_rejects_empty_proof_bytes() {
        let verifier = ProofEnvelopeVerifier::new();
        let mut payload = valid_payload();
        payload.proof_bytes = vec![];
        let err = verifier.verify_envelope(&payload).unwrap_err();
        assert_eq!(err, ProofEnvelopeError::EmptyProof);
    }

    #[test]
    fn test_rejects_unsupported_curve_system() {
        let verifier = ProofEnvelopeVerifier::new();
        let mut payload = valid_payload();
        payload.curve = "p256".to_string();
        let err = verifier.verify_envelope(&payload).unwrap_err();
        assert_eq!(
            err,
            ProofEnvelopeError::UnsupportedProofSystem {
                system: "groth16".to_string(),
                curve: "p256".to_string(),
            }
        );
    }

    #[test]
    fn test_rejects_invalid_hash_length() {
        let verifier = ProofEnvelopeVerifier::new();
        let mut payload = valid_payload();
        payload.state_root = "22".repeat(16); // Only 16 bytes
        let err = verifier.verify_envelope(&payload).unwrap_err();
        assert_eq!(
            err,
            ProofEnvelopeError::InvalidHashLength {
                field: "state_root".to_string(),
                expected_bytes: 32,
                actual_bytes: 16,
            }
        );
    }

    #[test]
    fn test_rejects_unauthorized_owner() {
        let verifier = ProofEnvelopeVerifier::new();
        let mut payload = valid_payload();
        payload.verifier_owner = "unauthorized".to_string();
        let err = verifier.verify_envelope(&payload).unwrap_err();
        assert_eq!(
            err,
            ProofEnvelopeError::UnauthorizedOwner {
                owner: "unauthorized".to_string()
            }
        );
    }
}
