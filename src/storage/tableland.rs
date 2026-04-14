//! [CON-69] Sovereign Sharding Persistence (Tableland Integration).
//! Bridges off-shore yield routing state to decentralized Tableland tables.

use crate::storage::Storage;
use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use reqwest::Client;

#[derive(Debug, Serialize, Deserialize)]
pub struct TablelandStateCommitment {
    pub table_id: String,
    pub query: String,
    pub timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TablelandWriteRequest {
    pub statement: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TablelandWriteResponse {
    pub hash: Option<String>,
    pub error: Option<String>,
}

/// [NEXUS-STATE-01] Tableland persistence layer.
pub struct TablelandAdapter {
    _storage: Arc<Storage>,
    base_url: String,
    client: Client,
}

impl TablelandAdapter {
    pub fn new(storage: Arc<Storage>, base_url: String) -> Self {
        Self {
            _storage: storage,
            base_url,
            client: Client::new(),
        }
    }

    /// Commit state to Tableland to bypass jurisdictional risks and ensure sovereign persistence.
    /// In production, this requires a valid Tableland private key to sign the transaction.
    /// For the current PoC, we implement the REST call to a Tableland gateway.
    pub async fn commit_state(
        &self,
        commitment: TablelandStateCommitment,
    ) -> anyhow::Result<String> {
        tracing::info!("Committing state to Tableland: {}", commitment.table_id);

        let url = format!("{}/api/v1/query", self.base_url.trim_end_matches('/'));

        let response = self.client
            .post(&url)
            .json(&TablelandWriteRequest {
                statement: commitment.query.clone(),
            })
            .send()
            .await
            .context("Failed to send request to Tableland")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("Tableland error: {}", error_text);
            return Err(anyhow!("Tableland error: {}", error_text));
        }

        let result: TablelandWriteResponse = response.json().await
            .context("Failed to parse Tableland response")?;

        if let Some(err) = result.error {
            return Err(anyhow!("Tableland execution error: {}", err));
        }

        let tx_hash = result.hash.ok_or_else(|| anyhow!("No transaction hash returned from Tableland"))?;

        tracing::info!("State committed to Tableland. Tx: {}", tx_hash);
        Ok(tx_hash)
    }
}
