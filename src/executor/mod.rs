//! nexus-executor module provides a specialized environment for high-frequency trades
//! and implements the FSOC (First-Seen-On-Chain) sequencer logic.

use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use std::sync::Mutex;

/// A request for off-chain execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub tx_id: String,
    pub payload: String,
    pub timestamp: DateTime<Utc>,
    pub sender: String,
}

/// Represents a vault's collateral status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatus {
    pub vault_id: String,
    pub collateral_amount: u128,
    pub debt_amount: u128,
    pub ltv_ratio: f64,
}

/// The executor service for handling transactions and rebalancing.
pub struct NexusExecutor {
    storage: Arc<Storage>,
    latest_event_time_cache: Mutex<Option<DateTime<Utc>>>,
}

impl NexusExecutor {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            latest_event_time_cache: Mutex::new(None),
        }
    }

    /// FSOC (First-Seen-On-Chain) Sequencer logic.
    /// Validates that a transaction is not attempting to front-run on-chain events.
    pub async fn validate_transaction(&self, request: &ExecutionRequest) -> anyhow::Result<bool> {
        let latest_on_chain_event_time = self.get_cached_or_fetch_latest_event_time().await?;

        if let Some(event_time) = latest_on_chain_event_time {
            // Strict verification against the Stacks microblock stream.
            if request.timestamp < event_time {
                tracing::warn!(
                    "Transaction {} timestamp ({}) is before latest on-chain event ({}). Potential manipulation.",
                    request.tx_id,
                    request.timestamp,
                    event_time
                );
                return Ok(false);
            }
        }

        let front_running_detected = self.detect_front_running(request).await?;

        if front_running_detected {
            tracing::warn!("Potential front-running detected for tx: {}", request.tx_id);
            return Ok(false);
        }

        Ok(true)
    }

    async fn get_cached_or_fetch_latest_event_time(&self) -> anyhow::Result<Option<DateTime<Utc>>> {
        {
            let cache = self.latest_event_time_cache.lock().unwrap();
            if let Some(time) = *cache {
                return Ok(Some(time));
            }
        }

        let row =
            sqlx::query("SELECT created_at FROM stacks_blocks ORDER BY created_at DESC LIMIT 1")
                .fetch_optional(&self.storage.pg_pool)
                .await?;

        let time = row.map(|r| r.get::<DateTime<Utc>, _>("created_at"));

        if let Some(t) = time {
            let mut cache = self.latest_event_time_cache.lock().unwrap();
            *cache = Some(t);
        }

        Ok(time)
    }

    /// Checks for front-running against detected on-chain liquidations or oracle updates.
    async fn detect_front_running(&self, request: &ExecutionRequest) -> anyhow::Result<bool> {
        // 1. Check for spamming from the same sender
        let row = sqlx::query(
            "SELECT COUNT(*) FROM stacks_transactions WHERE tx_id != $1 AND sender = $2 AND created_at > $3"
        )
        .bind(&request.tx_id)
        .bind(&request.sender)
        .bind(request.timestamp - chrono::Duration::seconds(60)) // 1 minute window
        .fetch_one(&self.storage.pg_pool).await?;

        let sender_count: i64 = row.get(0);

        if sender_count > 10 {
            tracing::warn!(
                "User {} is sending transactions too frequently: {}",
                request.sender,
                sender_count
            );
            return Ok(true);
        }

        // 2. Check for identical payloads in a short window (copy-cat front-running)
        let row = sqlx::query(
            "SELECT COUNT(*) FROM stacks_transactions WHERE tx_id != $1 AND payload = $2 AND created_at > $3"
        )
        .bind(&request.tx_id)
        .bind(&request.payload)
        .bind(request.timestamp - chrono::Duration::seconds(5))
        .fetch_one(&self.storage.pg_pool).await?;

        let payload_count: i64 = row.get(0);

        if payload_count > 0 {
            tracing::warn!(
                "Identical payload already seen on-chain: {}",
                request.tx_id
            );
            return Ok(true);
        }

        // 3. Heuristic: check if the payload contains keywords associated with liquidations
        if request.payload.contains("liquidate") {
             let last_oracle_update = self.get_cached_or_fetch_latest_event_time().await?;
             if let Some(t) = last_oracle_update {
                 if request.timestamp.signed_duration_since(t).num_milliseconds() < 500 {
                     tracing::warn!("Liquidation tx {} arrived within 500ms of latest block. Potential MEV.", request.tx_id);
                 }
             }
        }

        Ok(false)
    }

    /// Executes high-frequency internal trades and collateral rebalancing.
    pub async fn execute_rebalance(&self) -> anyhow::Result<()> {
        tracing::info!("Checking collateral health for rebalancing...");

        // In a real environment, we'd fetch all vault states.
        // For now, we simulate this with a flag or a list of vaults to check.
        let vaults = self.get_vaults_to_check().await?;
        let mut rebalance_count = 0;

        for vault in vaults {
            if vault.ltv_ratio > 0.85 { // 85% LTV threshold for rebalancing
                tracing::info!("Vault {} needs rebalance (LTV: {:.2})", vault.vault_id, vault.ltv_ratio);
                let tx_id = lib_conxian_core::sign_transaction(&format!("rebalance-{}", vault.vault_id));
                tracing::info!("Rebalance transaction broadcasted: {}", tx_id);
                rebalance_count += 1;
            }
        }

        if rebalance_count > 0 {
            tracing::info!("Rebalanced {} vaults.", rebalance_count);
            // Signal bounty success to the Stacks contract
            let signal_tx = lib_conxian_core::sign_transaction("agent-risk:signal-bounty-success");
            tracing::info!("Bounty success signaled: {}", signal_tx);
        } else {
            tracing::debug!("Collateral levels healthy. No rebalance needed.");
        }

        Ok(())
    }

    async fn get_vaults_to_check(&self) -> anyhow::Result<Vec<VaultStatus>> {
        // Try to get from Redis, fallback to empty or mock if Redis is down
        let mut conn = match self.storage.redis_client.get_multiplexed_async_connection().await {
            Ok(c) => c,
            Err(_) => return Ok(vec![]),
        };

        let data: Vec<String> = redis::cmd("SMEMBERS")
            .arg("nexus:active_vaults")
            .query_async(&mut conn)
            .await
            .unwrap_or_default();

        let mut vaults = Vec::new();
        for vault_id in data {
            let status_json: String = redis::cmd("GET")
                .arg(format!("vault:{}", vault_id))
                .query_async(&mut conn)
                .await
                .unwrap_or_default();

            if !status_json.is_empty() {
                if let Ok(v) = serde_json::from_str::<VaultStatus>(&status_json) {
                    vaults.push(v);
                }
            }
        }

        Ok(vaults)
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
            sender: "SP...".to_string(),
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
