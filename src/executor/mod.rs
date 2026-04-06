//! nexus-executor module provides a specialized environment for high-frequency trades
//! and implements the FSOC (First-Seen-On-Chain) sequencer logic.

use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    #[tracing::instrument(skip(self))]
    pub async fn validate_transaction(&self, request: &ExecutionRequest) -> anyhow::Result<bool> {
        let latest_on_chain_event_time = self.get_cached_or_fetch_latest_event_time().await?;

        if let Some(event_time) = latest_on_chain_event_time {
            if request.timestamp < event_time {
                let reason = format!(
                    "Timestamp {} is before latest on-chain event {}",
                    request.timestamp, event_time
                );
                tracing::warn!("Transaction {} rejected: {}", request.tx_id, reason);
                self.log_revenue_intelligence(request).await.ok();
                self.log_mev_attempt(request, &reason).await.ok();
                return Ok(false);
            }
        }

        if let Some(reason) = self.detect_front_running(request).await? {
            tracing::warn!(
                "Potential front-running detected for tx {}: {}",
                request.tx_id,
                reason
            );
            self.log_mev_attempt(request, &reason).await.ok();
            return Ok(false);
        }

        if self.detect_sandwich_attack(request).await? {
            let reason = "Potential Sandwich Attack detected (burst of swaps/liquidity)";
            tracing::warn!("Transaction {} rejected: {}", request.tx_id, reason);
            self.log_revenue_intelligence(request).await.ok();
            self.log_mev_attempt(request, reason).await.ok();
            return Ok(false);
        }

        Ok(true)
    }

    async fn log_mev_attempt(
        &self,
        request: &ExecutionRequest,
        reason: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO mev_audit_log (tx_id, sender, reason, payload) VALUES ($1, $2, $3, $4)",
        )
        .bind(&request.tx_id)
        .bind(&request.sender)
        .bind(reason)
        .bind(&request.payload)
        .execute(&self.storage.pg_pool)
        .await?;
        Ok(())
    }

    /// [CON-68] Revenue Intelligence mapping.
    /// Maps every signature/settlement to a Customer ID for ARR/MRR/Churn metrics.
    async fn log_revenue_intelligence(&self, request: &ExecutionRequest) -> anyhow::Result<()> {
        let customer_id = format!(
            "cust_{}",
            hex::encode(&Sha256::digest(request.sender.as_bytes())[..8])
        );
        tracing::debug!("Mapping revenue for customer: {}", customer_id);

        // Update volume metrics in Redis for revenue tracking.
        if let Ok(mut conn) = self
            .storage
            .redis_client
            .get_multiplexed_async_connection()
            .await
        {
            let _: redis::RedisResult<()> = redis::cmd("HINCRBY")
                .arg(format!("metrics:customer:{}", customer_id))
                .arg("total_volume")
                .arg(1)
                .query_async(&mut conn)
                .await;
        }

        Ok(())
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

    async fn detect_front_running(
        &self,
        request: &ExecutionRequest,
    ) -> anyhow::Result<Option<String>> {
        let row = sqlx::query(
            "SELECT COUNT(*) FROM stacks_transactions WHERE tx_id != $1 AND sender = $2 AND created_at > $3"
        )
        .bind(&request.tx_id)
        .bind(&request.sender)
        .bind(request.timestamp - chrono::Duration::seconds(60))
        .fetch_one(&self.storage.pg_pool).await?;

        let sender_count: i64 = row.get(0);

        if sender_count > 10 {
            return Ok(Some(format!(
                "Sender spamming detected: {} txs in 60s",
                sender_count
            )));
        }

        let row = sqlx::query(
            "SELECT COUNT(*) FROM stacks_transactions WHERE tx_id != $1 AND payload = $2 AND created_at > $3"
        )
        .bind(&request.tx_id)
        .bind(&request.payload)
        .bind(request.timestamp - chrono::Duration::seconds(5))
        .fetch_one(&self.storage.pg_pool).await?;

        let payload_count: i64 = row.get(0);

        if payload_count > 0 {
            return Ok(Some(
                "Identical payload already seen on-chain (copy-cat attempt)".to_string(),
            ));
        }

        if request.payload.contains("liquidate") {
            let last_oracle_update = self.get_cached_or_fetch_latest_event_time().await?;
            if let Some(t) = last_oracle_update {
                if request
                    .timestamp
                    .signed_duration_since(t)
                    .num_milliseconds()
                    < 200
                {
                    return Ok(Some(
                        "Liquidation arrival within 200ms of latest block (high MEV probability)"
                            .to_string(),
                    ));
                }
            }
        }

        Ok(None)
    }

    async fn detect_sandwich_attack(&self, request: &ExecutionRequest) -> anyhow::Result<bool> {
        let window = chrono::Duration::seconds(2);

        let row = sqlx::query(
            "SELECT COUNT(*) FROM stacks_transactions WHERE sender = $1 AND created_at BETWEEN $2 AND $3"
        )
        .bind(&request.sender)
        .bind(request.timestamp - window)
        .bind(request.timestamp + window)
        .fetch_one(&self.storage.pg_pool).await?;

        let burst_count: i64 = row.get(0);

        if burst_count >= 2
            && (request.payload.contains("swap") || request.payload.contains("liquidity"))
        {
            return Ok(true);
        }

        Ok(false)
    }

    /// Executes high-frequency internal trades and collateral rebalancing.
    pub async fn execute_rebalance(&self) -> anyhow::Result<()> {
        tracing::info!("Checking collateral health for rebalancing...");

        let fx_rate = self.get_latest_fx_rate("STX").await.unwrap_or(1.5); // Default to 1.5 USD/STX if missing
        let vaults = self.get_vaults_from_storage().await?;
        let mut rebalance_count = 0;

        for mut vault in vaults {
            let collateral_value_usd = (vault.collateral_amount as f64) * fx_rate / 1_000_000.0;
            let debt_value_usd = (vault.debt_amount as f64) / 1_000_000.0;

            if collateral_value_usd > 0.0 {
                vault.ltv_ratio = debt_value_usd / collateral_value_usd;
            }

            if vault.ltv_ratio > 0.85 {
                tracing::info!(
                    "Vault {} needs rebalance (LTV: {:.2}, STX: ${:.2})",
                    vault.vault_id,
                    vault.ltv_ratio,
                    fx_rate
                );
                let tx_id =
                    lib_conxian_core::sign_transaction(&format!("rebalance-{}", vault.vault_id));
                tracing::info!("Rebalance transaction broadcasted: {}", tx_id);
                rebalance_count += 1;
            }
        }

        if rebalance_count > 0 {
            tracing::info!("Rebalanced {} vaults.", rebalance_count);
            let signal_tx = lib_conxian_core::sign_transaction("agent-risk:signal-bounty-success");
            tracing::info!("Bounty success signaled: {}", signal_tx);
        } else {
            tracing::debug!(
                "Collateral levels healthy (STX: ${:.2}). No rebalance needed.",
                fx_rate
            );
        }

        Ok(())
    }

    async fn get_latest_fx_rate(&self, symbol: &str) -> Option<f64> {
        let row =
            sqlx::query("SELECT rates FROM oracle_fx_history ORDER BY timestamp DESC LIMIT 1")
                .fetch_optional(&self.storage.pg_pool)
                .await
                .ok()??;

        let rates: serde_json::Value = row.get("rates");
        rates.get(symbol).and_then(|v| v.as_f64())
    }

    async fn get_vaults_from_storage(&self) -> anyhow::Result<Vec<VaultStatus>> {
        let mut conn = match self
            .storage
            .redis_client
            .get_multiplexed_async_connection()
            .await
        {
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
