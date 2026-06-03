pub mod rgb;

use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::sync::{Arc, Mutex};

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub tx_id: String,
    pub payload: String,
    pub timestamp: DateTime<Utc>,
    pub sender: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VaultStatus {
    pub vault_id: String,
    pub collateral_amount: u64,
    pub debt_amount: u64,
    pub ltv_ratio: f64,
}

pub struct NexusExecutor {
    pub storage: Arc<Storage>,
    pub latest_event_time_cache: Mutex<Option<DateTime<Utc>>>,
    pub rgb_adapter: rgb::RGBAdapter,
}

impl NexusExecutor {
    pub fn new(storage: Arc<Storage>, rgb_mode: rgb::RGBRolloutMode, known_contracts: std::collections::HashSet<String>) -> Self {
        let rgb_adapter = rgb::RGBAdapter::with_known_contracts(rgb_mode, known_contracts);
        Self {
            storage,
            latest_event_time_cache: Mutex::new(None),
            rgb_adapter,
        }
    }

    pub async fn submit(&self, request: ExecutionRequest) -> anyhow::Result<String> {
        if !self.validate_transaction(&request).await? {
            anyhow::bail!("Transaction validation failed");
        }

        sqlx::query(
            "INSERT INTO me_audit_log (tx_id, payload_hash, sender, arrival_time)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(&request.tx_id)
        .bind(hex::encode(Sha256::digest(request.payload.as_bytes())))
        .bind(&request.sender)
        .bind(request.timestamp)
        .execute(&self.storage.pg_pool)
        .await?;

        tracing::info!("Transaction {} accepted by FSOC sequencer", request.tx_id);
        Ok(request.tx_id)
    }

    pub async fn validate_transaction(&self, request: &ExecutionRequest) -> anyhow::Result<bool> {
        if let Some(event_time) = self.get_cached_or_fetch_latest_event_time().await? {
            if request.timestamp <= event_time {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn get_cached_or_fetch_latest_event_time(&self) -> anyhow::Result<Option<DateTime<Utc>>> {
        {
            let cache = self.latest_event_time_cache.lock().unwrap();
            if let Some(t) = *cache {
                return Ok(Some(t));
            }
        }

        let row = sqlx::query("SELECT MAX(arrival_time) as last_time FROM me_audit_log")
            .fetch_one(&self.storage.pg_pool)
            .await?;

        let last_time: Option<DateTime<Utc>> = row.get("last_time");
        if let Some(t) = last_time {
            let mut cache = self.latest_event_time_cache.lock().unwrap();
            *cache = Some(t);
        }
        Ok(last_time)
    }

    pub async fn execute_rebalance(&self) -> anyhow::Result<()> {
        Ok(())
    }

    pub async fn get_latest_fx_rate(&self, symbol: &str) -> Option<f64> {
        let row =
            sqlx::query("SELECT rates FROM oracle_fx_history ORDER BY timestamp DESC LIMIT 1")
                .fetch_optional(&self.storage.pg_pool)
                .await
                .ok()??;

        let rates: serde_json::Value = row.get("rates");
        rates.get(symbol).and_then(|v| v.as_f64())
    }

    pub async fn get_vaults_from_storage(&self) -> anyhow::Result<Vec<VaultStatus>> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_execution_request_serialization() {
        let req = ExecutionRequest {
            tx_id: "tx123".to_string(),
            payload: "data".to_string(),
            timestamp: Utc::now(),
            sender: "sender".to_string(),
        };
        let serialized = serde_json::to_string(&req).unwrap();
        let deserialized: ExecutionRequest = serde_json::from_str(&serialized).unwrap();
        assert_eq!(req.tx_id, deserialized.tx_id);
    }

    #[test]
    fn test_vault_status_serialization() {
        let v = VaultStatus {
            vault_id: "v1".to_string(),
            collateral_amount: 1000,
            debt_amount: 800,
            ltv_ratio: 0.8,
        };
        let s = serde_json::to_string(&v).unwrap();
        let v2: VaultStatus = serde_json::from_str(&s).unwrap();
        assert_eq!(v.vault_id, v2.vault_id);
    }
}
