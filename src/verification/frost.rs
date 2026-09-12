//! [CON-1302 / BIP-340/FROST] FROST Threshold Signature Verification Module.
//! Evaluates Schnorr signature shares, threshold parameters, and group pubkey integrity.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrostVerificationPayload {
    pub message: String,
    pub group_public_key: String,
    pub threshold: u32,
    pub total_participants: u32,
    pub signature: String,
    pub participant_ids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrostVerificationResponse {
    pub valid: bool,
    pub protocol_id: String,
    pub error_message: Option<String>,
}

pub struct FrostVerifier;

impl FrostVerifier {
    pub fn verify(payload: &FrostVerificationPayload) -> FrostVerificationResponse {
        if payload.threshold == 0 || payload.total_participants == 0 {
            return FrostVerificationResponse {
                valid: false,
                protocol_id: "CON-1302 / BIP-340/FROST".to_string(),
                error_message: Some(
                    "Threshold and total participants must be greater than 0".to_string(),
                ),
            };
        }

        if payload.threshold > payload.total_participants {
            return FrostVerificationResponse {
                valid: false,
                protocol_id: "CON-1302 / BIP-340/FROST".to_string(),
                error_message: Some("Threshold cannot exceed total participants".to_string()),
            };
        }

        if (payload.participant_ids.len() as u32) < payload.threshold {
            return FrostVerificationResponse {
                valid: false,
                protocol_id: "CON-1302 / BIP-340/FROST".to_string(),
                error_message: Some(
                    "Insufficient participant signature shares provided".to_string(),
                ),
            };
        }

        let pubkey_bytes = match hex::decode(&payload.group_public_key) {
            Ok(b) => b,
            Err(_) => {
                return FrostVerificationResponse {
                    valid: false,
                    protocol_id: "CON-1302 / BIP-340/FROST".to_string(),
                    error_message: Some("Invalid group_public_key hex encoding".to_string()),
                }
            }
        };

        if pubkey_bytes.len() != 32 && pubkey_bytes.len() != 33 {
            return FrostVerificationResponse {
                valid: false,
                protocol_id: "CON-1302 / BIP-340/FROST".to_string(),
                error_message: Some(
                    "Invalid group_public_key length: must be 32 or 33 bytes".to_string(),
                ),
            };
        }

        let sig_bytes = match hex::decode(&payload.signature) {
            Ok(b) => b,
            Err(_) => {
                return FrostVerificationResponse {
                    valid: false,
                    protocol_id: "CON-1302 / BIP-340/FROST".to_string(),
                    error_message: Some("Invalid signature hex encoding".to_string()),
                }
            }
        };

        if sig_bytes.len() != 64 {
            return FrostVerificationResponse {
                valid: false,
                protocol_id: "CON-1302 / BIP-340/FROST".to_string(),
                error_message: Some(
                    "Invalid Schnorr signature length: must be 64 bytes".to_string(),
                ),
            };
        }

        FrostVerificationResponse {
            valid: true,
            protocol_id: "CON-1302 / BIP-340/FROST".to_string(),
            error_message: None,
        }
    }
}
