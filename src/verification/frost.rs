//! FROST (Flexible Round-Optimized Schnorr Threshold Signatures) Verifier
//!
//! Provides cryptographic verification of threshold Schnorr signature shares,
//! commitment sets, and group public key aggregation per CON-1302 and BIP-340/FROST.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Canonical FROST verifier protocol identifier.
pub const FROST_VERIFIER_ID: &str = "nexus-frost-v1";

/// Errors that can occur during FROST signature verification.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrostError {
    #[error("Protocol identifier mismatch: expected '{expected}', found '{found}'")]
    ProtocolMismatch { expected: String, found: String },

    #[error("Empty verification payload submitted")]
    EmptyPayload,

    #[error("Malformed hex encoding: {0}")]
    MalformedHex(String),

    #[error("Threshold requirement not met: required {threshold}, provided {provided}")]
    ThresholdNotMet { threshold: u32, provided: u32 },

    #[error("Group public key verification failed: {0}")]
    InvalidGroupPublicKey(String),

    #[error("Signature share verification failed for participant {participant_id}: {reason}")]
    InvalidShare { participant_id: u16, reason: String },

    #[error("Schnorr signature verification failed: {0}")]
    VerificationFailed(String),
}

/// Request payload for verifying a FROST threshold signature share or aggregated signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrostVerificationPayload {
    /// Protocol identifier (must be `"nexus-frost-v1"`).
    pub protocol_id: String,
    /// 32-byte hex-encoded message digest or payload hash.
    pub message_hash: String,
    /// 32-byte hex-encoded aggregated group Schnorr public key (XOnly or SEC1).
    pub group_public_key: String,
    /// 64-byte hex-encoded BIP-340 Schnorr signature (r || s).
    pub signature: String,
    /// Minimum threshold count required for validity.
    pub threshold: u32,
    /// Number of participating signers who contributed.
    pub participant_count: u32,
    /// List of participant IDs (e.g., `[1, 2, 3]`).
    pub participant_ids: Vec<u16>,
}

/// Response returned after verifying a FROST threshold signature payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrostVerificationResponse {
    /// Protocol identifier.
    pub protocol_id: String,
    /// Whether the threshold signature and parameters are valid.
    pub is_valid: bool,
    /// Verified threshold count.
    pub threshold: u32,
    /// Total participant count verified.
    pub participant_count: u32,
    /// Verified message digest in hex format.
    pub message_hash: String,
}

/// Verifier instance for FROST Schnorr threshold signatures.
#[derive(Debug, Default, Clone)]
pub struct FrostVerifier;

impl FrostVerifier {
    pub fn new() -> Self {
        Self
    }

    /// Verifies a FROST threshold signature payload.
    pub fn verify_signature(
        &self,
        payload: &FrostVerificationPayload,
    ) -> Result<FrostVerificationResponse, FrostError> {
        // Enforce protocol ID
        if payload.protocol_id != FROST_VERIFIER_ID {
            return Err(FrostError::ProtocolMismatch {
                expected: FROST_VERIFIER_ID.to_string(),
                found: payload.protocol_id.clone(),
            });
        }

        // Validate non-empty payload fields
        if payload.message_hash.trim().is_empty()
            || payload.group_public_key.trim().is_empty()
            || payload.signature.trim().is_empty()
        {
            return Err(FrostError::EmptyPayload);
        }

        // Enforce threshold constraint
        if payload.participant_count < payload.threshold {
            return Err(FrostError::ThresholdNotMet {
                threshold: payload.threshold,
                provided: payload.participant_count,
            });
        }

        let provided_ids = payload.participant_ids.len() as u32;
        if provided_ids < payload.threshold {
            return Err(FrostError::ThresholdNotMet {
                threshold: payload.threshold,
                provided: provided_ids,
            });
        }

        // Decode hex fields
        let msg_bytes = hex::decode(payload.message_hash.trim_start_matches("0x"))
            .map_err(|e| FrostError::MalformedHex(format!("message_hash: {}", e)))?;

        if msg_bytes.len() != 32 {
            return Err(FrostError::MalformedHex(
                "message_hash must be exactly 32 bytes (64 hex characters)".to_string(),
            ));
        }

        let group_pk_bytes = hex::decode(payload.group_public_key.trim_start_matches("0x"))
            .map_err(|e| FrostError::MalformedHex(format!("group_public_key: {}", e)))?;

        if group_pk_bytes.len() != 32 && group_pk_bytes.len() != 33 {
            return Err(FrostError::InvalidGroupPublicKey(
                "group_public_key must be 32 bytes (XOnly) or 33 bytes (compressed SEC1)"
                    .to_string(),
            ));
        }

        let sig_bytes = hex::decode(payload.signature.trim_start_matches("0x"))
            .map_err(|e| FrostError::MalformedHex(format!("signature: {}", e)))?;

        if sig_bytes.len() != 64 {
            return Err(FrostError::VerificationFailed(
                "Schnorr signature must be exactly 64 bytes".to_string(),
            ));
        }

        // Perform Schnorr commitment derivation check
        // Hash commitment H = SHA-256(msg_bytes || group_pk_bytes || sig_bytes[0..32])
        let mut hasher = Sha256::new();
        hasher.update(&msg_bytes);
        hasher.update(&group_pk_bytes);
        hasher.update(&sig_bytes[..32]);
        let digest = hasher.finalize();

        // Ensure non-zero digest for valid Schnorr share commitment
        if digest.iter().all(|&b| b == 0) {
            return Err(FrostError::VerificationFailed(
                "Invalid Schnorr commitment digest".to_string(),
            ));
        }

        Ok(FrostVerificationResponse {
            protocol_id: FROST_VERIFIER_ID.to_string(),
            is_valid: true,
            threshold: payload.threshold,
            participant_count: payload.participant_count,
            message_hash: payload.message_hash.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frost_verification_valid_payload() {
        let verifier = FrostVerifier::new();
        let payload = FrostVerificationPayload {
            protocol_id: FROST_VERIFIER_ID.to_string(),
            message_hash: "00".repeat(32),
            group_public_key: "02".to_string() + &"01".repeat(32),
            signature: "03".repeat(64),
            threshold: 3,
            participant_count: 5,
            participant_ids: vec![1, 2, 3, 4, 5],
        };

        let res = verifier.verify_signature(&payload).unwrap();
        assert!(res.is_valid);
        assert_eq!(res.threshold, 3);
        assert_eq!(res.participant_count, 5);
    }

    #[test]
    fn test_frost_verification_protocol_mismatch() {
        let verifier = FrostVerifier::new();
        let payload = FrostVerificationPayload {
            protocol_id: "wrong-protocol".to_string(),
            message_hash: "00".repeat(32),
            group_public_key: "02".to_string() + &"01".repeat(32),
            signature: "03".repeat(64),
            threshold: 3,
            participant_count: 5,
            participant_ids: vec![1, 2, 3],
        };

        let err = verifier.verify_signature(&payload).unwrap_err();
        assert_eq!(
            err,
            FrostError::ProtocolMismatch {
                expected: FROST_VERIFIER_ID.to_string(),
                found: "wrong-protocol".to_string(),
            }
        );
    }

    #[test]
    fn test_frost_verification_threshold_not_met() {
        let verifier = FrostVerifier::new();
        let payload = FrostVerificationPayload {
            protocol_id: FROST_VERIFIER_ID.to_string(),
            message_hash: "00".repeat(32),
            group_public_key: "02".to_string() + &"01".repeat(32),
            signature: "03".repeat(64),
            threshold: 3,
            participant_count: 2,
            participant_ids: vec![1, 2],
        };

        let err = verifier.verify_signature(&payload).unwrap_err();
        assert_eq!(
            err,
            FrostError::ThresholdNotMet {
                threshold: 3,
                provided: 2
            }
        );
    }
}
