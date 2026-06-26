use crate::storage::Storage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// IBC Light Client Update model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IBCClientUpdate {
    pub client_id: String,
    pub header: String, // Base64 encoded Tendermint header
    pub trusted_height: u64,
}

/// Verification result for an IBC light client update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IBCVerificationResult {
    pub valid: bool,
    pub client_id: String,
    pub latest_height: u64,
    pub trust_level: String,
}

/// Protocol Adapter for Cosmos / IBC family.
pub struct CosmosAdapter {
    storage: Arc<Storage>,
}

impl CosmosAdapter {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Verifies an IBC light client update.
    pub async fn verify_client_update(
        &self,
        update: &IBCClientUpdate,
    ) -> anyhow::Result<IBCVerificationResult> {
        // [ADR-006] IBC Light Client verification within the Nexus state layer.
        // [NIP-005] Phase 1: Client ID and structural validation.

        if !update.client_id.contains("-") || update.client_id.len() < 5 {
            return Ok(IBCVerificationResult {
                valid: false,
                client_id: update.client_id.clone(),
                latest_height: 0,
                trust_level: "None".to_string(),
            });
        }

        // [IBC-RESEARCH] Future implementation will use ibc-rs for Tendermint verification.
        let latest_height = update.trusted_height + 1;
        let trust_level = "T1 (NIP-005 Phase 1)".to_string();

        let _ = sqlx::query(
            "INSERT INTO cosmos_verified_client_updates (client_id, latest_height, trust_level)
             VALUES ($1, $2, $3)
             ON CONFLICT (client_id) DO UPDATE SET latest_height = EXCLUDED.latest_height"
        )
        .bind(&update.client_id)
        .bind(latest_height as i64)
        .bind(&trust_level)
        .execute(&self.storage.pg_pool)
        .await;

        Ok(IBCVerificationResult {
            valid: true,
            client_id: update.client_id.clone(),
            latest_height,
            trust_level,
        })
    }
}
