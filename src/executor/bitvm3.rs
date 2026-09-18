//! BitVM3 Garbled-Circuit Fraud Proof Verifier.
//!
//! Provides fast-dispute assertion handling, garbled circuit gate commitment inspection,
//! and wire label evaluation to reduce on-chain fraud challenge size to ~200 bytes.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Supported logic gate types for BitVM3 garbled circuit evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bitvm3GateType {
    And,
    Xor,
    Nand,
}

/// Wire label commitment in a garbled circuit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireLabel {
    pub wire_id: u32,
    pub label: String,
    pub value: bool,
}

/// Commitment for a specific garbled gate within a circuit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateCommitment {
    pub gate_id: u32,
    pub gate_type: Bitvm3GateType,
    pub garbled_table_hashes: Vec<String>,
}

/// Dispute assertion asserting a specific gate equivocation or miscomputation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisputeAssertion {
    pub disputed_gate_id: u32,
    pub input_labels: Vec<WireLabel>,
    pub claimed_output_label: WireLabel,
    pub expected_output_digest: String,
}

/// Payload for verifying a BitVM3 garbled-circuit fraud proof dispute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bitvm3VerificationPayload {
    pub circuit_id: String,
    pub challenge_nonce: String,
    pub gate_commitments: Vec<GateCommitment>,
    pub dispute: DisputeAssertion,
}

/// Response returned after evaluating a BitVM3 fraud proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bitvm3VerificationResponse {
    pub is_valid_fraud_proof: bool,
    pub circuit_id: String,
    pub disputed_gate_id: u32,
    pub proof_digest: String,
    pub message: String,
}

/// Errors during BitVM3 fraud proof verification.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Bitvm3VerificationError {
    #[error("Circuit ID cannot be empty")]
    EmptyCircuitId,
    #[error("Gate commitments cannot be empty")]
    EmptyGateCommitments,
    #[error("Invalid challenge nonce length")]
    InvalidChallengeNonce,
    #[error("Disputed gate {0} not found in circuit commitments")]
    GateNotFound(u32),
    #[error("Insufficient input wire labels for gate evaluation (requires at least 2)")]
    InsufficientInputLabels,
    #[error("Malformed wire label or digest hex")]
    MalformedHex,
    #[error("Garbled table entry hash mismatch for gate {0}")]
    GarbledTableHashMismatch(u32),
    #[error("No fraud detected: claimed output matches valid gate computation")]
    NoFraudDetected,
}

/// BitVM3 garbled-circuit fraud proof verifier.
pub struct Bitvm3Verifier;

impl Bitvm3Verifier {
    /// Verifies a BitVM3 garbled-circuit fraud proof dispute payload.
    pub fn verify_fraud_proof(
        payload: &Bitvm3VerificationPayload,
    ) -> Result<Bitvm3VerificationResponse, Bitvm3VerificationError> {
        if payload.circuit_id.trim().is_empty() {
            return Err(Bitvm3VerificationError::EmptyCircuitId);
        }
        if payload.gate_commitments.is_empty() {
            return Err(Bitvm3VerificationError::EmptyGateCommitments);
        }

        let challenge_nonce_bytes = hex::decode(&payload.challenge_nonce)
            .map_err(|_| Bitvm3VerificationError::MalformedHex)?;
        if challenge_nonce_bytes.len() != 32 {
            return Err(Bitvm3VerificationError::InvalidChallengeNonce);
        }

        let dispute = &payload.dispute;
        let gate = payload
            .gate_commitments
            .iter()
            .find(|g| g.gate_id == dispute.disputed_gate_id)
            .ok_or(Bitvm3VerificationError::GateNotFound(
                dispute.disputed_gate_id,
            ))?;

        if dispute.input_labels.len() < 2 {
            return Err(Bitvm3VerificationError::InsufficientInputLabels);
        }

        // 1. Evaluate expected logic gate output
        let in0 = dispute.input_labels[0].value;
        let in1 = dispute.input_labels[1].value;
        let expected_bool = match gate.gate_type {
            Bitvm3GateType::And => in0 && in1,
            Bitvm3GateType::Xor => in0 ^ in1,
            Bitvm3GateType::Nand => !(in0 && in1),
        };

        // 2. Decode wire labels for entry hash derivation
        let label0_bytes = hex::decode(&dispute.input_labels[0].label)
            .map_err(|_| Bitvm3VerificationError::MalformedHex)?;
        let label1_bytes = hex::decode(&dispute.input_labels[1].label)
            .map_err(|_| Bitvm3VerificationError::MalformedHex)?;
        let claimed_output_bytes = hex::decode(&dispute.claimed_output_label.label)
            .map_err(|_| Bitvm3VerificationError::MalformedHex)?;

        // 3. Compute garbled table entry hash: SHA256(gate_id || challenge_nonce || label0 || label1 || claimed_output_label)
        let mut hasher = Sha256::new();
        hasher.update(gate.gate_id.to_be_bytes());
        hasher.update(&challenge_nonce_bytes);
        hasher.update(&label0_bytes);
        hasher.update(&label1_bytes);
        hasher.update(&claimed_output_bytes);
        let entry_hash = hex::encode(hasher.finalize());

        // 4. Check if entry_hash exists in the gate's garbled_table_hashes
        if !gate
            .garbled_table_hashes
            .iter()
            .any(|h| h.eq_ignore_ascii_case(&entry_hash))
        {
            return Err(Bitvm3VerificationError::GarbledTableHashMismatch(
                gate.gate_id,
            ));
        }

        // 5. Check for fraud: if claimed_output_label.value != expected_bool OR claimed_output_label.label != expected_output_digest
        let value_mismatch = dispute.claimed_output_label.value != expected_bool;
        let label_mismatch = !dispute
            .claimed_output_label
            .label
            .eq_ignore_ascii_case(&dispute.expected_output_digest);

        let is_valid_fraud_proof = value_mismatch || label_mismatch;

        if !is_valid_fraud_proof {
            return Err(Bitvm3VerificationError::NoFraudDetected);
        }

        // Compute overall proof digest
        let mut proof_hasher = Sha256::new();
        proof_hasher.update(payload.circuit_id.as_bytes());
        proof_hasher.update(gate.gate_id.to_be_bytes());
        proof_hasher.update(&challenge_nonce_bytes);
        proof_hasher.update(entry_hash.as_bytes());
        let proof_digest = hex::encode(proof_hasher.finalize());

        Ok(Bitvm3VerificationResponse {
            is_valid_fraud_proof: true,
            circuit_id: payload.circuit_id.clone(),
            disputed_gate_id: gate.gate_id,
            proof_digest,
            message: format!(
                "BitVM3 fraud proven for gate {} in circuit {}: value_mismatch={}, label_mismatch={}",
                gate.gate_id, payload.circuit_id, value_mismatch, label_mismatch
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_nonce_hex() -> String {
        hex::encode([7u8; 32])
    }

    fn sample_digest_hex(byte: u8) -> String {
        hex::encode([byte; 32])
    }

    fn compute_entry_hash(
        gate_id: u32,
        nonce_hex: &str,
        l0_hex: &str,
        l1_hex: &str,
        out_hex: &str,
    ) -> String {
        let nonce_bytes = hex::decode(nonce_hex).unwrap();
        let l0 = hex::decode(l0_hex).unwrap();
        let l1 = hex::decode(l1_hex).unwrap();
        let out = hex::decode(out_hex).unwrap();

        let mut hasher = Sha256::new();
        hasher.update(gate_id.to_be_bytes());
        hasher.update(&nonce_bytes);
        hasher.update(&l0);
        hasher.update(&l1);
        hasher.update(&out);
        hex::encode(hasher.finalize())
    }

    #[test]
    fn test_bitvm3_fraud_proof_success_value_mismatch() {
        let gate_id = 42;
        let nonce = sample_nonce_hex();
        let l0 = sample_digest_hex(1);
        let l1 = sample_digest_hex(2);
        let claimed_out_label = sample_digest_hex(3);
        let expected_correct_digest = sample_digest_hex(4);

        let entry_hash = compute_entry_hash(gate_id, &nonce, &l0, &l1, &claimed_out_label);

        let payload = Bitvm3VerificationPayload {
            circuit_id: "bitvm3-circuit-alpha".to_string(),
            challenge_nonce: nonce,
            gate_commitments: vec![GateCommitment {
                gate_id,
                gate_type: Bitvm3GateType::And,
                garbled_table_hashes: vec![entry_hash],
            }],
            dispute: DisputeAssertion {
                disputed_gate_id: gate_id,
                input_labels: vec![
                    WireLabel {
                        wire_id: 1,
                        label: l0,
                        value: true,
                    },
                    WireLabel {
                        wire_id: 2,
                        label: l1,
                        value: true,
                    },
                ],
                claimed_output_label: WireLabel {
                    wire_id: 3,
                    label: claimed_out_label,
                    value: false, // AND(true, true) is true, but claimed false => Fraud!
                },
                expected_output_digest: expected_correct_digest,
            },
        };

        let res = Bitvm3Verifier::verify_fraud_proof(&payload).unwrap();
        assert!(res.is_valid_fraud_proof);
        assert_eq!(res.disputed_gate_id, 42);
        assert_eq!(res.circuit_id, "bitvm3-circuit-alpha");
    }

    #[test]
    fn test_bitvm3_rejects_empty_circuit() {
        let payload = Bitvm3VerificationPayload {
            circuit_id: "".to_string(),
            challenge_nonce: sample_nonce_hex(),
            gate_commitments: vec![],
            dispute: DisputeAssertion {
                disputed_gate_id: 1,
                input_labels: vec![],
                claimed_output_label: WireLabel {
                    wire_id: 3,
                    label: sample_digest_hex(1),
                    value: true,
                },
                expected_output_digest: sample_digest_hex(1),
            },
        };

        let err = Bitvm3Verifier::verify_fraud_proof(&payload).unwrap_err();
        assert_eq!(err, Bitvm3VerificationError::EmptyCircuitId);
    }

    #[test]
    fn test_bitvm3_rejects_missing_gate() {
        let payload = Bitvm3VerificationPayload {
            circuit_id: "circuit-1".to_string(),
            challenge_nonce: sample_nonce_hex(),
            gate_commitments: vec![GateCommitment {
                gate_id: 10,
                gate_type: Bitvm3GateType::Xor,
                garbled_table_hashes: vec![sample_digest_hex(9)],
            }],
            dispute: DisputeAssertion {
                disputed_gate_id: 99, // missing
                input_labels: vec![
                    WireLabel {
                        wire_id: 1,
                        label: sample_digest_hex(1),
                        value: true,
                    },
                    WireLabel {
                        wire_id: 2,
                        label: sample_digest_hex(2),
                        value: false,
                    },
                ],
                claimed_output_label: WireLabel {
                    wire_id: 3,
                    label: sample_digest_hex(3),
                    value: true,
                },
                expected_output_digest: sample_digest_hex(3),
            },
        };

        let err = Bitvm3Verifier::verify_fraud_proof(&payload).unwrap_err();
        assert_eq!(err, Bitvm3VerificationError::GateNotFound(99));
    }
}
