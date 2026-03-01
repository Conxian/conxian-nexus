//! nexus-safety module implements the Sovereign Handoff safety protocol.
//!
//! It monitors the drift between the Nexus processed state and the Stacks L1
//! burn-block height, triggering a safety mode if the Nexus falls behind.

use crate::storage::Storage;
use reqwest::Client;
use serde_json::Value;
use sqlx::Row;
use std::sync::Arc;
use tokio::time::{self, Duration};

/// Monitors the health and sync status of the Nexus.
pub struct NexusSafety {
    storage: Arc<Storage>,
    max_drift: u64,
    rpc_url: String,
    gateway_url: String,
    http_client: Client,
}

impl NexusSafety {
    /// Creates a new safety monitor with a default max drift of 2 blocks.
    pub fn new(storage: Arc<Storage>, rpc_url: String, gateway_url: String) -> Self {
        Self {
            storage,
            max_drift: 2,
            rpc_url,
            gateway_url,
            http_client: Client::new(),
        }
    }

    /// Runs the heartbeat monitor loop.
    pub async fn run_heartbeat(&self) -> anyhow::Result<()> {
        let mut interval = time::interval(Duration::from_secs(10));
        tracing::info!(
            "Starting NexusSafety heartbeat (max_drift: {} blocks, RPC: {}, Gateway: {})...",
            self.max_drift,
            self.rpc_url,
            self.gateway_url
        );

        loop {
            interval.tick().await;
            if let Err(e) = self.check_health().await {
                tracing::error!("Safety heartbeat error: {}", e);
            }
            if let Err(e) = self.ingest_gateway_telemetry().await {
                tracing::error!("Gateway telemetry ingestion error: {}", e);
            }
        }
    }

    /// Ingests telemetry from the Gateway and triggers safety mode if failure rates spike.
    async fn ingest_gateway_telemetry(&self) -> anyhow::Result<()> {
        let url = format!("{}/api/v1/state", self.gateway_url);
        let resp = self.http_client.get(&url).send().await?;
        
        if !resp.status().is_success() {
            tracing::warn!("Failed to fetch Gateway state: {}", resp.status());
            return Ok(());
        }

        let json: Value = resp.json().await?;
        
        let success_count = json["metrics"]["verification_success"].as_u64().unwrap_or(0);
        let failure_count = json["metrics"]["verification_failure"].as_u64().unwrap_or(0);
        
        // Define a simple circuit breaker logic based on failures
        let total_verifications = success_count + failure_count;
        
        if total_verifications > 100 {
            let failure_rate = (failure_count as f64) / (total_verifications as f64);
            // If more than 10% of verifications are failing, trigger an infrastructure-level safety alert
            if failure_rate > 0.10 {
                tracing::error!(
                    "Gateway Circuit Breaker Triggered! Failure Rate: {:.2}% (Success: {}, Failures: {})",
                    failure_rate * 100.0,
                    success_count,
                    failure_count
                );
                
                // We reuse the existing safety mode broadcast but flag it as a telemetry alert
                self.trigger_safety_mode(999).await?; // 999 is a synthetic drift indicating a telemetry fault
            }
        }
        
        Ok(())
    }

    /// Checks the health by comparing local processed height with external L1 height.
    async fn check_health(&self) -> anyhow::Result<()> {
        let current_burn_height = self.get_external_burn_height().await?;
        let processed_height = self.get_processed_height().await?;

        let delta = Self::calculate_drift(current_burn_height, processed_height);

        if delta > self.max_drift {
            tracing::error!(
                "Sovereign Handoff Triggered! Delta: {} blocks (L1: {}, Local: {})",
                delta,
                current_burn_height,
                processed_height
            );
            self.trigger_safety_mode(delta).await?;
        } else {
            tracing::debug!("Nexus health check passed. Drift: {} blocks", delta);
            self.clear_safety_mode_if_needed(delta).await?;
        }

        Ok(())
    }

    pub fn calculate_drift(current: u64, processed: u64) -> u64 {
        current.saturating_sub(processed)
    }

    async fn get_external_burn_height(&self) -> anyhow::Result<u64> {
        // Real implementation: calls Stacks node RPC.
        let url = format!("{}/extended/v1/block?limit=1", self.rpc_url);
        let resp = self.http_client.get(&url).send().await?;
        let json: Value = resp.json().await?;

        let height = json["results"][0]["height"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse block height from Stacks RPC"))?;

        Ok(height)
    }

    async fn get_processed_height(&self) -> anyhow::Result<u64> {
        let row = sqlx::query("SELECT MAX(height) as max_height FROM stacks_blocks")
            .fetch_one(&self.storage.pg_pool)
            .await?;

        let max_height: Option<i64> = row.get("max_height");
        Ok(max_height.unwrap_or(0) as u64)
    }

    /// Triggers Safety Mode and broadcasts it via Redis.
    async fn trigger_safety_mode(&self, delta: u64) -> anyhow::Result<()> {
        let mut conn = self
            .storage
            .redis_client
            .get_multiplexed_async_connection()
            .await?;

        redis::pipe()
            .atomic()
            .cmd("SET")
            .arg("nexus:safety_mode")
            .arg(true)
            .cmd("SET")
            .arg("nexus:drift")
            .arg(delta)
            .cmd("PUBLISH")
            .arg("nexus:events")
            .arg("safety_mode_triggered")
            .query_async::<_, ()>(&mut conn)
            .await?;

        Ok(())
    }

    async fn clear_safety_mode_if_needed(&self, _delta: u64) -> anyhow::Result<()> {
        let mut conn = self
            .storage
            .redis_client
            .get_multiplexed_async_connection()
            .await?;
        let is_safety_mode: bool = redis::cmd("GET")
            .arg("nexus:safety_mode")
            .query_async(&mut conn)
            .await
            .unwrap_or(false);

        if is_safety_mode {
            tracing::info!("System recovered. Clearing Safety Mode.");
            redis::pipe()
                .atomic()
                .cmd("DEL")
                .arg("nexus:safety_mode")
                .cmd("DEL")
                .arg("nexus:drift")
                .cmd("PUBLISH")
                .arg("nexus:events")
                .arg("safety_mode_cleared")
                .query_async::<_, ()>(&mut conn)
                .await?;
        }
        Ok(())
    }

    /// Provides status and proof for "Direct Withdrawal Tenure".
    pub async fn get_direct_exit_status(&self, user_address: &str) -> anyhow::Result<String> {
        let mut conn = self
            .storage
            .redis_client
            .get_multiplexed_async_connection()
            .await?;
        let is_safety_mode: bool = redis::cmd("GET")
            .arg("nexus:safety_mode")
            .query_async(&mut conn)
            .await
            .unwrap_or(false);

        if is_safety_mode {
            Ok(format!(
                "User {}: Eligible for Direct Withdrawal (Safety Mode Active)",
                user_address
            ))
        } else {
            Ok(format!(
                "User {}: System healthy, use standard exit paths",
                user_address
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_drift() {
        assert_eq!(NexusSafety::calculate_drift(100, 98), 2);
        assert_eq!(NexusSafety::calculate_drift(100, 102), 0);
        assert_eq!(NexusSafety::calculate_drift(100, 100), 0);
    }
}
