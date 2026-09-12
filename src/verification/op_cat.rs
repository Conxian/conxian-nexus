//! [CON-1303 / BIP-347] OP_CAT Recursive Covenant Policy Verifier.
//! Simulates Taproot covenant script concatenation, stack bounds (520 bytes max), and recursion depth limits.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpCatVerificationPayload {
    pub script_elements_hex: Vec<String>,
    pub max_stack_size_bytes: usize,
    pub recursion_depth_limit: u32,
    pub target_state_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpCatVerificationResponse {
    pub valid: bool,
    pub protocol_id: String,
    pub concatenated_length: usize,
    pub error_message: Option<String>,
}

pub struct OpCatVerifier;

impl OpCatVerifier {
    pub fn verify(payload: &OpCatVerificationPayload) -> OpCatVerificationResponse {
        let max_stack = if payload.max_stack_size_bytes == 0 {
            520
        } else {
            payload.max_stack_size_bytes
        };

        let max_depth = if payload.recursion_depth_limit == 0 {
            16
        } else {
            payload.recursion_depth_limit
        };

        if payload.script_elements_hex.is_empty() {
            return OpCatVerificationResponse {
                valid: false,
                protocol_id: "CON-1303 / BIP-347 OP_CAT".to_string(),
                concatenated_length: 0,
                error_message: Some(
                    "No script elements provided for OP_CAT concatenation".to_string(),
                ),
            };
        }

        let mut total_length = 0;
        let mut concatenated_bytes = Vec::new();

        for (idx, elem_hex) in payload.script_elements_hex.iter().enumerate() {
            let bytes = match hex::decode(elem_hex) {
                Ok(b) => b,
                Err(_) => {
                    return OpCatVerificationResponse {
                        valid: false,
                        protocol_id: "CON-1303 / BIP-347 OP_CAT".to_string(),
                        concatenated_length: 0,
                        error_message: Some(format!(
                            "Invalid hex encoding at element index {}",
                            idx
                        )),
                    }
                }
            };

            total_length += bytes.len();
            if total_length > max_stack {
                return OpCatVerificationResponse {
                    valid: false,
                    protocol_id: "CON-1303 / BIP-347 OP_CAT".to_string(),
                    concatenated_length: total_length,
                    error_message: Some(format!(
                        "Stack element size {} exceeds maximum stack bound of {} bytes",
                        total_length, max_stack
                    )),
                };
            }

            concatenated_bytes.extend(bytes);
        }

        if payload.script_elements_hex.len() as u32 > max_depth {
            return OpCatVerificationResponse {
                valid: false,
                protocol_id: "CON-1303 / BIP-347 OP_CAT".to_string(),
                concatenated_length: total_length,
                error_message: Some(format!(
                    "Recursion depth {} exceeds policy limit of {}",
                    payload.script_elements_hex.len(),
                    max_depth
                )),
            };
        }

        OpCatVerificationResponse {
            valid: true,
            protocol_id: "CON-1303 / BIP-347 OP_CAT".to_string(),
            concatenated_length: total_length,
            error_message: None,
        }
    }
}
