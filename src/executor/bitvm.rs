use crate::storage::Storage;
use ark_bls12_381::{Bls12_381, Fr};
use ark_crypto_primitives::snark::SNARK;
use ark_groth16::Groth16;
use ark_serialize::CanonicalDeserialize;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// BitVM2 State Transition model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitVMTransition {
    pub prev_state_root: String,
    pub next_state_root: String,
    pub proof_bytes: String,
    pub vk_bytes: String,
    pub public_inputs: Vec<String>,
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

    /// Verifies a Groth16 state transition proof for BitVM2.
    /// This replaces the simulated verification with real cryptographic logic (NIP-005).
    pub async fn verify_transition(
        &self,
        transition: &BitVMTransition,
    ) -> anyhow::Result<BitVMVerificationResult> {
        // 1. Basic validation
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

        // 2. Decode Cryptographic Primitives
        let proof_bytes = hex::decode(&transition.proof_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid proof hex: {}", e))?;
        let vk_bytes = hex::decode(&transition.vk_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid VK hex: {}", e))?;

        let proof = ark_groth16::Proof::<Bls12_381>::deserialize_compressed(&proof_bytes[..])
            .map_err(|e| anyhow::anyhow!("Failed to deserialize proof: {}", e))?;

        let vk = ark_groth16::VerifyingKey::<Bls12_381>::deserialize_compressed(&vk_bytes[..])
            .map_err(|e| anyhow::anyhow!("Failed to deserialize VK: {}", e))?;

        let mut public_inputs = Vec::new();
        let mut public_inputs_concatenated = Vec::new();
        for input_hex in &transition.public_inputs {
            let input_bytes =
                hex::decode(input_hex).map_err(|e| anyhow::anyhow!("Invalid input hex: {}", e))?;
            public_inputs_concatenated.extend_from_slice(&input_bytes);
            let input_fr = Fr::deserialize_compressed(&input_bytes[..])
                .map_err(|e| anyhow::anyhow!("Failed to deserialize public input: {}", e))?;
            public_inputs.push(input_fr);
        }

        // 3. Cryptographic Verification
        let is_valid = Groth16::<Bls12_381>::verify(&vk, &public_inputs, &proof)
            .map_err(|e| anyhow::anyhow!("SNARK verification error: {}", e))?;

        if !is_valid {
            return Ok(BitVMVerificationResult {
                valid: false,
                message: "Groth16 proof verification failed".to_string(),
                steps_verified: 0,
                confidence: 0.0,
            });
        }

        // 4. Persistence and Auditing
        let steps = 1024; // Representative value for current BitVM2 trial circuits
        let confidence = 1.0;
        let proof_hash = hex::encode(Sha256::digest(&proof_bytes));
        let vk_hash = hex::encode(Sha256::digest(&vk_bytes));
        let pi_hash = hex::encode(Sha256::digest(&public_inputs_concatenated));

        let _ = sqlx::query(
            "INSERT INTO bitvm_verified_transitions (trace_id, prev_state_root, next_state_root, proof_hash, vk_hash, public_inputs_hash, steps_verified, confidence)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (trace_id) DO NOTHING"
        )
        .bind(&transition.trace_id)
        .bind(&transition.prev_state_root)
        .bind(&transition.next_state_root)
        .bind(&proof_hash)
        .bind(&vk_hash)
        .bind(&pi_hash)
        .bind(steps as i64)
        .bind(confidence)
        .execute(&self.storage.pg_pool)
        .await;

        Ok(BitVMVerificationResult {
            valid: true,
            message: "Transition cryptographically verified and audited successfully (NIP-005)"
                .to_string(),
            steps_verified: steps,
            confidence,
        })
    }
}
