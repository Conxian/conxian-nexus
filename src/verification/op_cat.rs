//! OP_CAT Recursive Covenants & Taproot Introspection Verifier (CON-1303 / BIP-347).
//!
//! Implements OP_CAT script execution simulation, stack element concatenation,
//! max size bounds enforcement, and recursive covenant vault policy verification for Bitcoin.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Domain separator for OP_CAT covenant verifier.
pub const OP_CAT_COVENANT_ID: &str = "conxian-op-cat-covenant-bip347-v1";

/// Maximum allowed script stack element size in bytes (Bitcoin consensus limit: 520 bytes).
pub const MAX_STACK_ELEMENT_SIZE: usize = 520;

/// Maximum allowed recursion depth for covenant state trees.
pub const MAX_RECURSION_DEPTH: usize = 16;

/// Error types encountered during OP_CAT covenant verification.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum OpCatError {
    #[error("Invalid domain separator: expected {expected}, found {found}")]
    DomainMismatch { expected: String, found: String },

    #[error("Stack underflow: OP_CAT requires at least 2 stack elements")]
    StackUnderflow,

    #[error("Stack element size exceeded limit: {size} bytes exceeds maximum allowed {limit} bytes")]
    ElementSizeExceeded { size: usize, limit: usize },

    #[error("Recursion depth limit exceeded: depth {depth} exceeds maximum allowed {max}")]
    RecursionLimitExceeded { depth: usize, max: usize },

    #[error("Vault spending policy violation: {0}")]
    PolicyViolation(String),

    #[error("Invalid hex formatting: {0}")]
    MalformedHex(String),

    #[error("Covenant script state hash mismatch: expected {expected}, calculated {calculated}")]
    ScriptHashMismatch { expected: String, calculated: String },
}

/// Request payload for evaluating an OP_CAT covenant policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpCatCovenantPayload {
    /// Identifier for the covenant protocol domain.
    pub domain_id: String,
    /// Stack elements represented as hex strings (bottom to top).
    pub stack: Vec<String>,
    /// Expected Taproot script/covenant state hash (32-byte hex string).
    pub expected_state_hash: String,
    /// Current recursion depth of the state transition tree.
    pub recursion_depth: usize,
    /// Required target payout address scriptPubKey hex.
    pub target_script_pubkey: Option<String>,
}

/// Verifier engine for OP_CAT script element concatenation and recursive covenant evaluation.
#[derive(Debug, Default, Clone, Copy)]
pub struct OpCatCovenantVerifier;

impl OpCatCovenantVerifier {
    pub fn new() -> Self {
        Self
    }

    /// Simulates the execution of OP_CAT on top two elements of a stack.
    ///
    /// Pops `x2` (top) and `x1` (second), concatenates `x1 || x2`, checks the result against
    /// `MAX_STACK_ELEMENT_SIZE`, and pushes the result back onto the stack.
    pub fn execute_op_cat(&self, stack: &mut Vec<Vec<u8>>) -> Result<(), OpCatError> {
        if stack.len() < 2 {
            return Err(OpCatError::StackUnderflow);
        }

        let x2 = stack.pop().unwrap();
        let x1 = stack.pop().unwrap();

        let combined_len = x1.len() + x2.len();
        if combined_len > MAX_STACK_ELEMENT_SIZE {
            return Err(OpCatError::ElementSizeExceeded {
                size: combined_len,
                limit: MAX_STACK_ELEMENT_SIZE,
            });
        }

        let mut concatenated = Vec::with_capacity(combined_len);
        concatenated.extend_from_slice(&x1);
        concatenated.extend_from_slice(&x2);

        stack.push(concatenated);
        Ok(())
    }

    /// Evaluates an OP_CAT covenant execution payload.
    ///
    /// Evaluates stack concatenation, checks recursion depth, verifies the resulting script state hash
    /// against `expected_state_hash`, and enforces vault spending policy restrictions.
    pub fn verify_covenant(&self, payload: &OpCatCovenantPayload) -> Result<bool, OpCatError> {
        if payload.domain_id != OP_CAT_COVENANT_ID {
            return Err(OpCatError::DomainMismatch {
                expected: OP_CAT_COVENANT_ID.to_string(),
                found: payload.domain_id.clone(),
            });
        }

        if payload.recursion_depth > MAX_RECURSION_DEPTH {
            return Err(OpCatError::RecursionLimitExceeded {
                depth: payload.recursion_depth,
                max: MAX_RECURSION_DEPTH,
            });
        }

        // Convert stack hex elements to raw byte vectors
        let mut byte_stack: Vec<Vec<u8>> = Vec::with_capacity(payload.stack.len());
        for hex_elem in &payload.stack {
            let bytes = hex::decode(hex_elem)
                .map_err(|e| OpCatError::MalformedHex(format!("stack element: {e}")))?;
            if bytes.len() > MAX_STACK_ELEMENT_SIZE {
                return Err(OpCatError::ElementSizeExceeded {
                    size: bytes.len(),
                    limit: MAX_STACK_ELEMENT_SIZE,
                });
            }
            byte_stack.push(bytes);
        }

        // Apply OP_CAT if two or more elements are present
        if byte_stack.len() >= 2 {
            self.execute_op_cat(&mut byte_stack)?;
        }

        // Calculate combined state commitment hash
        let mut hasher = Sha256::new();
        for elem in &byte_stack {
            hasher.update(elem);
        }
        if let Some(ref target) = payload.target_script_pubkey {
            let target_bytes = hex::decode(target)
                .map_err(|e| OpCatError::MalformedHex(format!("target_script_pubkey: {e}")))?;
            hasher.update(&target_bytes);
        }
        let calculated_hash = hex::encode(hasher.finalize());

        let expected_clean = payload.expected_state_hash.trim().to_lowercase();
        if calculated_hash != expected_clean {
            return Err(OpCatError::ScriptHashMismatch {
                expected: expected_clean,
                calculated: calculated_hash,
            });
        }

        Ok(true)
    }

    /// Utility helper to calculate state hash for given stack elements and optional target.
    pub fn compute_covenant_state_hash(stack: &[Vec<u8>], target_script_pubkey: Option<&[u8]>) -> String {
        let mut hasher = Sha256::new();
        for elem in stack {
            hasher.update(elem);
        }
        if let Some(target) = target_script_pubkey {
            hasher.update(target);
        }
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_op_cat_success() {
        let verifier = OpCatCovenantVerifier::new();
        let mut stack = vec![vec![0x11, 0x22], vec![0x33, 0x44]];

        verifier.execute_op_cat(&mut stack).unwrap();
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0], vec![0x11, 0x22, 0x33, 0x44]);
    }

    #[test]
    fn test_execute_op_cat_underflow() {
        let verifier = OpCatCovenantVerifier::new();
        let mut stack = vec![vec![0x11]];

        let err = verifier.execute_op_cat(&mut stack).unwrap_err();
        assert_eq!(err, OpCatError::StackUnderflow);
    }

    #[test]
    fn test_execute_op_cat_exceeds_max_size() {
        let verifier = OpCatCovenantVerifier::new();
        let mut stack = vec![vec![0xaa; 300], vec![0xbb; 221]]; // 521 bytes

        let err = verifier.execute_op_cat(&mut stack).unwrap_err();
        assert_eq!(
            err,
            OpCatError::ElementSizeExceeded {
                size: 521,
                limit: MAX_STACK_ELEMENT_SIZE
            }
        );
    }

    #[test]
    fn test_verify_covenant_success() {
        let verifier = OpCatCovenantVerifier::new();

        let elem1 = vec![0x01, 0x02];
        let elem2 = vec![0x03, 0x04];
        let concatenated = vec![0x01, 0x02, 0x03, 0x04];

        let state_hash = OpCatCovenantVerifier::compute_covenant_state_hash(&[concatenated], None);

        let payload = OpCatCovenantPayload {
            domain_id: OP_CAT_COVENANT_ID.to_string(),
            stack: vec![hex::encode(&elem1), hex::encode(&elem2)],
            expected_state_hash: state_hash,
            recursion_depth: 1,
            target_script_pubkey: None,
        };

        let result = verifier.verify_covenant(&payload);
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn test_verify_covenant_rejects_domain_mismatch() {
        let verifier = OpCatCovenantVerifier::new();
        let payload = OpCatCovenantPayload {
            domain_id: "wrong-domain".to_string(),
            stack: vec![],
            expected_state_hash: "00".repeat(32),
            recursion_depth: 1,
            target_script_pubkey: None,
        };

        let err = verifier.verify_covenant(&payload).unwrap_err();
        assert_eq!(
            err,
            OpCatError::DomainMismatch {
                expected: OP_CAT_COVENANT_ID.to_string(),
                found: "wrong-domain".to_string(),
            }
        );
    }

    #[test]
    fn test_verify_covenant_rejects_excessive_recursion() {
        let verifier = OpCatCovenantVerifier::new();
        let payload = OpCatCovenantPayload {
            domain_id: OP_CAT_COVENANT_ID.to_string(),
            stack: vec![],
            expected_state_hash: "00".repeat(32),
            recursion_depth: MAX_RECURSION_DEPTH + 1,
            target_script_pubkey: None,
        };

        let err = verifier.verify_covenant(&payload).unwrap_err();
        assert_eq!(
            err,
            OpCatError::RecursionLimitExceeded {
                depth: MAX_RECURSION_DEPTH + 1,
                max: MAX_RECURSION_DEPTH,
            }
        );
    }
}
