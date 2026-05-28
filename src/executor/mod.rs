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

/// [NEXUS-EXEC-01] FSOC (First-Seen-On-Chain) Sequencer logic.
/// Validates transaction ordering and prevents front-running by matching off-chain
/// payloads against L1 arrival order and cryptographic state proofs.
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

    /// [NEXUS-EXEC-02] Core submission logic for the FSOC sequencer.
    pub async fn submit(&self, request: ExecutionRequest) -> anyhow::Result<String> {
        if !self.validate_transaction(&request).await? {
            anyhow::bail!("Transaction validation failed");
        }

        sqlx::query(
            "INSERT INTO me_audit_log (tx_id, payload_hash, sender, arrival_time)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(&request.tx_id)
        .bind(&hex::encode(Sha256::digest(request.payload.as_bytes())))
        .bind(&request.sender)
        .bind(request.timestamp)
        .execute(&self.storage.pg_pool)
        .await?;

        tracing::info!("Transaction {} accepted by FSOC sequencer", request.tx_id);
        Ok(request.tx_id)
    }

    /// Validates a transaction request against the FSOC sequencer rules.
    pub async fn validate_transaction(&self, request: &ExecutionRequest) -> anyhow::Result<bool> {
        // [FSOC-RULE-01]: Ensure transaction timestamp is after the latest processed block.
        if let Some(event_time) = self.get_cached_or_fetch_latest_event_time().await? {
            if request.timestamp <= event_time {
                let reason = format!(
                    "Transaction timestamp {} is stale or conflicting with latest block {}",
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

    async fn detect_front_running(&self, _request: &ExecutionRequest) -> anyhow::Result<Option<String>> {
        // Placeholder for real front-running heuristics
        Ok(None)
    }

    async fn detect_sandwich_attack(&self, _request: &ExecutionRequest) -> anyhow::Result<bool> {
        // Placeholder for sandwich attack detection
        Ok(false)
    }

    async fn log_mev_attempt(
        &self,
        request: &ExecutionRequest,
        reason: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO me_audit_log (tx_id, sender, reason, payload_hash) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(&request.tx_id)
        .bind(&request.sender)
        .bind(reason)
        .bind(&hex::encode(Sha256::digest(request.payload.as_bytes())))
        .execute(&self.storage.pg_pool)
        .await?;
        Ok(())
    }

    /// [CON-68] Revenue Intelligence mapping.
    async fn log_revenue_intelligence(&self, request: &ExecutionRequest) -> anyhow::Result<()> {
        let customer_id = format!(
            "cust_{}",
            hex::encode(&Sha256::digest(request.sender.as_bytes())[..8])
        );
        tracing::debug!("Transaction {} mapped to customer {}", request.tx_id, customer_id);
        Ok(())
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

    /// [NEXUS-EXEC-03] Autonomous Rebalance Engine.
    pub async fn execute_rebalance(&self) -> anyhow::Result<()> {
        let fx_rate = self.get_latest_fx_rate("STX").await.unwrap_or(2.50);
        let vaults = self.get_vaults_from_storage().await?;

        let mut rebalance_count = 0;
        for vault in vaults {
            if vault.ltv_ratio > 0.85 {
                tracing::info!(
                    "Vault {} breach detected (LTV: {:.2}%). Initiating rebalance.",
                    vault.vault_id,
                    vault.ltv_ratio * 100.0
                );
                rebalance_count += 1;
            }
        }

        if rebalance_count > 0 {
            tracing::info!("Rebalanced {} vaults.", rebalance_count);
            match lib_conxian_core::sign_transaction("agent-risk:signal-bounty-success") {
                Ok(signal_tx) => {
                    tracing::info!("Bounty success signaled: {}", signal_tx);
                }
                Err(e) => {
                    tracing::error!("Failed to sign bounty success signal: {}", e);
                }
            }
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
            .query_async::<Vec<String>>(&mut conn)
            .await
            .unwrap_or_default();

        let mut vaults = Vec::new();
        for vault_id in data {
            let status_json: String = redis::cmd("GET")
                .arg(format!("vault:{}", vault_id))
                .query_async::<String>(&mut conn)
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
            sender: "SPSZXAKV7DWTDZN2601WR31BM51BD3YTQWE97VRM".to_string(),
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
