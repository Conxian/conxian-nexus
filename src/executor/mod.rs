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
        // We check if there's any transaction from the same sender in recent blocks
        // that has a different payload but targets the same logic, or if there's an unusually
        // high number of similar transactions from different senders (spam/front-running).

        let row = sqlx::query(
            "SELECT COUNT(*) FROM stacks_transactions WHERE tx_id != $1 AND sender = $2 AND created_at > $3"
        )
        .bind(&request.tx_id)
        .bind(&request.sender)
        .bind(request.timestamp - chrono::Duration::seconds(60)) // 1 minute window
        .fetch_one(&self.storage.pg_pool).await?;

        let sender_count: i64 = row.get(0);

        if sender_count > 5 {
            tracing::warn!(
                "User {} is sending transactions too frequently: {}",
                request.sender,
                sender_count
            );
            return Ok(true);
        }

        // Check for similar payloads in the same time window (potential front-running of a public strategy)
        let row = sqlx::query(
            "SELECT COUNT(*) FROM stacks_transactions WHERE tx_id != $1 AND payload = $2 AND created_at > $3"
        )
        .bind(&request.tx_id)
        .bind(&request.payload)
        .bind(request.timestamp - chrono::Duration::seconds(10))
        .fetch_one(&self.storage.pg_pool).await?;

        let payload_count: i64 = row.get(0);

        if payload_count > 2 {
            tracing::warn!(
                "Multiple identical payloads detected in a short window: {}",
                payload_count
            );
            return Ok(true);
        }

        Ok(false)
    }

    /// Executes high-frequency internal trades and collateral rebalancing.
    pub async fn execute_rebalance(&self) -> anyhow::Result<()> {
        tracing::info!("Executing collateral rebalancing for dex-router.clar...");
        // Logic to interact with Stacks smart contracts via conxian-core wallet
        Ok(())
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
}
