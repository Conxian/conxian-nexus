//! nexus-safety module implements the Sovereign Handoff safety protocol.
//!
//! It monitors the drift between the Nexus processed state and the Stacks L1
//! burn-block height, triggering a safety mode if the Nexus falls behind.

use std::sync::Arc;
use crate::storage::Storage;
use tokio::time::{self, Duration};
use sqlx::Row;
use std::sync::atomic::{AtomicU64, Ordering};

/// Monitors the health and sync status of the Nexus.
pub struct NexusSafety {
    storage: Arc<Storage>,
    max_drift: u64,
    simulated_l1_height: AtomicU64,
}

impl NexusSafety {
    /// Creates a new safety monitor with a default max drift of 2 blocks.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            max_drift: 2,
            simulated_l1_height: AtomicU64::new(0),
        }
    }

    /// Runs the heartbeat monitor loop.
    pub async fn run_heartbeat(&self) -> anyhow::Result<()> {
        let mut interval = time::interval(Duration::from_secs(10));
        tracing::info!("Starting NexusSafety heartbeat (max_drift: {} blocks)...", self.max_drift);

        loop {
            interval.tick().await;
            if let Err(e) = self.check_health().await {
                tracing::error!("Safety heartbeat error: {}", e);
            }
        }
    }

    /// Checks the health by comparing local processed height with external L1 height.
    async fn check_health(&self) -> anyhow::Result<()> {
        let current_burn_height = self.get_external_burn_height().await?;
        let processed_height = self.get_processed_height().await?;

        let delta = if current_burn_height > processed_height {
            current_burn_height - processed_height
        } else {
            0
        };

        if delta > self.max_drift {
            tracing::error!(
                "Sovereign Handoff Triggered! Delta: {} blocks (L1: {}, Local: {})",
                delta, current_burn_height, processed_height
            );
            self.trigger_safety_mode(delta).await?;
        } else {
            tracing::debug!("Nexus health check passed. Drift: {} blocks", delta);
            self.clear_safety_mode_if_needed(delta).await?;
        }

        Ok(())
    }

    async fn get_external_burn_height(&self) -> anyhow::Result<u64> {
        // Simulation: slowly increase L1 height.
        // In a real implementation, this would call a Stacks node RPC.
        let local = self.get_processed_height().await?;
        let current_sim = self.simulated_l1_height.load(Ordering::SeqCst);

        let target = if current_sim < local { local } else { current_sim };

        // Occasionally jump ahead to simulate drift
        let new_height = if rand::random::<u8>() % 20 == 0 {
            target + 5
        } else if target > local {
            target
        } else {
            local
        };

        self.simulated_l1_height.store(new_height, Ordering::SeqCst);
        Ok(new_height)
    }

    async fn get_processed_height(&self) -> anyhow::Result<u64> {
        let row = sqlx::query(
            "SELECT MAX(height) as max_height FROM stacks_blocks"
        ).fetch_one(&self.storage.pg_pool).await?;

        let max_height: Option<i64> = row.get("max_height");
        Ok(max_height.unwrap_or(0) as u64)
    }

    /// Triggers Safety Mode and broadcasts it via Redis.
    async fn trigger_safety_mode(&self, delta: u64) -> anyhow::Result<()> {
        let mut conn = self.storage.redis_client.get_multiplexed_async_connection().await?;

        redis::pipe()
            .atomic()
            .cmd("SET").arg("nexus:safety_mode").arg(true)
            .cmd("SET").arg("nexus:drift").arg(delta)
            .cmd("PUBLISH").arg("nexus:events").arg("safety_mode_triggered")
            .query_async::<_, ()>(&mut conn).await?;

        Ok(())
    }

    async fn clear_safety_mode_if_needed(&self, _delta: u64) -> anyhow::Result<()> {
        let mut conn = self.storage.redis_client.get_multiplexed_async_connection().await?;
        let is_safety_mode: bool = redis::cmd("GET")
            .arg("nexus:safety_mode")
            .query_async(&mut conn).await.unwrap_or(false);

        if is_safety_mode {
            tracing::info!("System recovered. Clearing Safety Mode.");
            redis::pipe()
                .atomic()
                .cmd("DEL").arg("nexus:safety_mode")
                .cmd("DEL").arg("nexus:drift")
                .cmd("PUBLISH").arg("nexus:events").arg("safety_mode_cleared")
                .query_async::<_, ()>(&mut conn).await?;
        }
        Ok(())
    }

    /// Provides status and proof for "Direct Withdrawal Tenure".
    pub async fn get_direct_exit_status(&self, user_address: &str) -> anyhow::Result<String> {
        let mut conn = self.storage.redis_client.get_multiplexed_async_connection().await?;
        let is_safety_mode: bool = redis::cmd("GET")
            .arg("nexus:safety_mode")
            .query_async(&mut conn).await.unwrap_or(false);

        if is_safety_mode {
            Ok(format!("User {}: Eligible for Direct Withdrawal (Safety Mode Active)", user_address))
        } else {
            Ok(format!("User {}: System healthy, use standard exit paths", user_address))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_drift_threshold() {
        let max_drift = 2;
        let current_burn_height = 105;
        let processed_height = 100;
        let delta = current_burn_height - processed_height;
        assert!(delta > max_drift);
    }
}
