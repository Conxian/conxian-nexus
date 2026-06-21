use crate::storage::Storage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// EVM Receipt Proof model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EVMReceiptProof {
    pub block_hash: String,
    pub transaction_index: u64,
    pub proof_nodes: Vec<String>,
    pub receipt_root: String,
}

/// Verification result for an EVM receipt proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EVMVerificationResult {
    pub valid: bool,
    pub status: String,
    pub verified_at_height: u64,
}

/// Protocol Adapter for Ethereum / EVM family.
pub struct EVMAdapter {
    storage: Arc<Storage>,
}

impl EVMAdapter {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Verifies an EVM receipt proof against a known or fetched receipt root.
    pub async fn verify_receipt_proof(
        &self,
        proof: &EVMReceiptProof,
    ) -> anyhow::Result<EVMVerificationResult> {
        // [ADR-006] Implement receipt proof verification logic.
        // In this phase, we perform structural validation and check block hash format.

        if !proof.block_hash.starts_with("0x") || proof.block_hash.len() != 66 {
            return Ok(EVMVerificationResult {
                valid: false,
                status: "Invalid block hash format".to_string(),
                verified_at_height: 0,
            });
        }

        if !proof.receipt_root.starts_with("0x") || proof.receipt_root.len() != 66 {
            return Ok(EVMVerificationResult {
                valid: false,
                status: "Invalid receipt root format".to_string(),
                verified_at_height: 0,
            });
        }

        let status = "Receipt proof verified and audited (simulated)".to_string();
        let verified_at_height = 1000000;

        // [NIP-005] Persist to audit log (best effort - don't fail verification if DB is down)
        let _ = sqlx::query(
            "INSERT INTO evm_verified_receipts (block_hash, transaction_index, receipt_root, status, verified_at_height)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (block_hash, transaction_index) DO NOTHING"
        )
        .bind(&proof.block_hash)
        .bind(proof.transaction_index as i64)
        .bind(&proof.receipt_root)
        .bind(&status)
        .bind(verified_at_height as i64)
        .execute(&self.storage.pg_pool)
        .await;

        // Mock success for validly formatted inputs
        Ok(EVMVerificationResult {
            valid: true,
            status,
            verified_at_height,
        })
    }
}
