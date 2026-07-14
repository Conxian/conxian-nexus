use serde::{Deserialize, Serialize};

/// Stacks / sBTC Transaction model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StacksTransaction {
    pub tx_id: String,
    pub block_height: u64,
    pub sender: String,
    pub amount_sbtc: u64,
}

/// Verification result for a Stacks / sBTC transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StacksVerificationResult {
    pub valid: bool,
    pub status: String,
    pub verified_at_height: u64,
}

/// Protocol Adapter for Stacks / sBTC family.
pub struct StacksAdapter;

impl Default for StacksAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl StacksAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Verifies a Stacks transaction (Pilot implementation).
    pub async fn verify_transaction(
        &self,
        tx: &StacksTransaction,
    ) -> anyhow::Result<StacksVerificationResult> {
        // [CON-709] Pilot implementation for Stacks + sBTC.
        // Performs basic structural validation.

        if tx.tx_id.is_empty() || !tx.tx_id.starts_with("0x") {
            return Ok(StacksVerificationResult {
                valid: false,
                status: "Invalid transaction ID format".to_string(),
                verified_at_height: 0,
            });
        }

        if tx.amount_sbtc == 0 {
            return Ok(StacksVerificationResult {
                valid: false,
                status: "Zero amount sBTC transaction".to_string(),
                verified_at_height: 0,
            });
        }

        // Mock success for validly formatted inputs (Pilot lane)
        Ok(StacksVerificationResult {
            valid: true,
            status: "Stacks transaction verified (Pilot)".to_string(),
            verified_at_height: tx.block_height,
        })
    }
}
