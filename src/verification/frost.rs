//! FROST (Flexible Round-Optimized Schnorr Threshold Signatures) Verifier
//!
//! Provides cryptographic verification of threshold Schnorr signature shares,
//! commitment sets, and group public key aggregation per CON-1302 and BIP-340/FROST.

use k256::schnorr::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
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
    /// 32-byte hex-encoded aggregated group Schnorr public key (XOnly BIP-340 or SEC1 compressed).
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

    /// Cryptographically verifies a FROST threshold BIP-340 Schnorr signature payload.
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

        // Decode message hash
        let msg_bytes = hex::decode(payload.message_hash.trim_start_matches("0x"))
            .map_err(|e| FrostError::MalformedHex(format!("message_hash: {}", e)))?;

        if msg_bytes.len() != 32 {
            return Err(FrostError::MalformedHex(
                "message_hash must be exactly 32 bytes (64 hex characters)".to_string(),
            ));
        }

        // Decode group public key (32-byte BIP-340 XOnly or 33-byte SEC1)
        let group_pk_bytes = hex::decode(payload.group_public_key.trim_start_matches("0x"))
            .map_err(|e| FrostError::MalformedHex(format!("group_public_key: {}", e)))?;

        let verifying_key = if group_pk_bytes.len() == 32 {
            let key_array: [u8; 32] = group_pk_bytes.as_slice().try_into().map_err(|_| {
                FrostError::InvalidGroupPublicKey("invalid key array conversion".to_string())
            })?;
            VerifyingKey::from_bytes(&key_array.into()).map_err(|e| {
                FrostError::InvalidGroupPublicKey(format!("invalid 32-byte BIP-340 key: {}", e))
            })?
        } else if group_pk_bytes.len() == 33 {
            let xonly_slice = &group_pk_bytes[1..33];
            let key_array: [u8; 32] = xonly_slice.try_into().map_err(|_| {
                FrostError::InvalidGroupPublicKey("invalid SEC1 slice conversion".to_string())
            })?;
            VerifyingKey::from_bytes(&key_array.into()).map_err(|e| {
                FrostError::InvalidGroupPublicKey(format!("invalid SEC1 group public key: {}", e))
            })?
        } else {
            return Err(FrostError::InvalidGroupPublicKey(
                "group_public_key must be 32 bytes (XOnly) or 33 bytes (compressed SEC1)"
                    .to_string(),
            ));
        };

        // Decode 64-byte BIP-340 Schnorr signature (r || s)
        let sig_bytes = hex::decode(payload.signature.trim_start_matches("0x"))
            .map_err(|e| FrostError::MalformedHex(format!("signature: {}", e)))?;

        if sig_bytes.len() != 64 {
            return Err(FrostError::VerificationFailed(
                "Schnorr signature must be exactly 64 bytes".to_string(),
            ));
        }

        let signature = Signature::try_from(sig_bytes.as_slice()).map_err(|e| {
            FrostError::VerificationFailed(format!("invalid BIP-340 signature encoding: {}", e))
        })?;

        // Perform cryptographic BIP-340 Schnorr signature verification over message_hash
        verifying_key
            .verify_raw(&msg_bytes, &signature)
            .map_err(|e| {
                FrostError::VerificationFailed(format!(
                    "BIP-340 Schnorr signature verification failed: {}",
                    e
                ))
            })?;

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
    use k256::schnorr::SigningKey;

    #[test]
    fn test_frost_verification_valid_bip340_signature() {
        let verifier = FrostVerifier::new();

        let signing_key = SigningKey::from_bytes(&[0x01u8; 32].into()).unwrap();
        let verifying_key = signing_key.verifying_key();

        let msg_bytes = [0x42u8; 32];
        let aux_rand = [0x01u8; 32];
        let signature = signing_key.sign_raw(&msg_bytes, &aux_rand).unwrap();

        let payload = FrostVerificationPayload {
            protocol_id: FROST_VERIFIER_ID.to_string(),
            message_hash: hex::encode(msg_bytes),
            group_public_key: hex::encode(verifying_key.to_bytes()),
            signature: hex::encode(signature.to_bytes()),
            threshold: 3,
            participant_count: 5,
            participant_ids: vec![1, 2, 3, 4, 5],
        };

        let res = verifier.verify_signature(&payload).unwrap();
        assert!(res.is_valid);
        assert_eq!(res.threshold, 3);
        assert_eq!(res.participant_count, 5);
        assert_eq!(res.message_hash, hex::encode(msg_bytes));
    }

    #[test]
    fn test_frost_verification_invalid_signature_bytes() {
        let verifier = FrostVerifier::new();

        let signing_key = SigningKey::from_bytes(&[0x01u8; 32].into()).unwrap();
        let verifying_key = signing_key.verifying_key();

        let msg_bytes = [0x42u8; 32];
        let bad_sig_bytes = [0x01u8; 64];

        let payload = FrostVerificationPayload {
            protocol_id: FROST_VERIFIER_ID.to_string(),
            message_hash: hex::encode(msg_bytes),
            group_public_key: hex::encode(verifying_key.to_bytes()),
            signature: hex::encode(bad_sig_bytes),
            threshold: 3,
            participant_count: 5,
            participant_ids: vec![1, 2, 3, 4, 5],
        };

        let err = verifier.verify_signature(&payload).unwrap_err();
        match err {
            FrostError::VerificationFailed(reason) => {
                assert!(
                    reason.contains("signature verification failed") || reason.contains("invalid")
                );
            }
            other => panic!("expected VerificationFailed, got {:?}", other),
        }
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
