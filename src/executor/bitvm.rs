use crate::storage::Storage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// BitVM2 State Transition model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitVMTransition {
    pub prev_state_root: String,
    pub next_state_root: String,
    pub proof_bytes: String,
    pub trace_id: String,
}

/// Verification result for a BitVM transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitVMVerificationResult {
    pub valid: bool,
    pub message: String,
    pub steps_verified: u64,
    pub confidence: f64,
}

/// Logic for BitVM2 verification and state simulation.
pub struct BitVMAdapter {
    storage: Arc<Storage>,
}

impl BitVMAdapter {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Simulates a state transition verification and persists the result.
    pub async fn verify_transition(
        &self,
        transition: &BitVMTransition,
    ) -> anyhow::Result<BitVMVerificationResult> {
        if transition.prev_state_root.len() != 66 || !transition.prev_state_root.starts_with("0x") {
            return Ok(BitVMVerificationResult {
                valid: false,
                message: "Invalid prev_state_root format".to_string(),
                steps_verified: 0,
                confidence: 0.0,
            });
        }

        if transition.next_state_root.len() != 66 || !transition.next_state_root.starts_with("0x") {
            return Ok(BitVMVerificationResult {
                valid: false,
                message: "Invalid next_state_root format".to_string(),
                steps_verified: 0,
                confidence: 0.0,
            });
        }

        // 1. Structural verification (simulated)
        let steps = 1024;
        let confidence = 0.99;
        let proof_hash = hex::encode(Sha256::digest(transition.proof_bytes.as_bytes()));

        // 2. Persist to audit log
        sqlx::query(
            "INSERT INTO bitvm_verified_transitions (trace_id, prev_state_root, next_state_root, proof_hash, steps_verified, confidence)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (trace_id) DO NOTHING"
        )
        .bind(&transition.trace_id)
        .bind(&transition.prev_state_root)
        .bind(&transition.next_state_root)
        .bind(&proof_hash)
        .bind(steps as i64)
        .bind(confidence)
        .execute(&self.storage.pg_pool)
        .await?;

        Ok(BitVMVerificationResult {
            valid: true,
            message: "Transition verified and audited successfully (simulated)".to_string(),
            steps_verified: steps,
            confidence,
        })
    }
}
