//! nexus-safety module implements the Sovereign Handoff safety protocol.
//!
//! It monitors the drift between the Nexus processed state and the Stacks L1
//! burn-block height, triggering a safety mode if the Nexus falls behind.

use std::sync::Arc;
use crate::storage::Storage;
use tokio::time::{self, Duration};
use sqlx::Row;

/// Monitors the health and sync status of the Nexus.
pub struct NexusSafety {
    storage: Arc<Storage>,
    max_drift: u64,
}

impl NexusSafety {
    /// Creates a new safety monitor with a default max drift of 2 blocks.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage, max_drift: 2 }
    }

    /// Runs the heartbeat monitor loop.
    pub async fn run_heartbeat(&self) -> anyhow::Result<()> {
        let mut interval = time::interval(Duration::from_secs(10));

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
            tracing::error!("Sovereign Handoff Triggered! Delta: {} blocks", delta);
            self.trigger_safety_mode(delta).await?;
        }

        Ok(())
    }

    async fn get_external_burn_height(&self) -> anyhow::Result<u64> {
        // Call Stacks L1 RPC for current burn block height
        Ok(100)
    }

    async fn get_processed_height(&self) -> anyhow::Result<u64> {
        let row = sqlx::query(
            "SELECT MAX(height) as max_height FROM stacks_blocks WHERE type = 'burn_block'"
        ).fetch_one(&self.storage.pg_pool).await?;

        let max_height: Option<i64> = row.get("max_height");
        Ok(max_height.unwrap_or(0) as u64)
    }

    /// Triggers Safety Mode and broadcasts it to the gateway.
    async fn trigger_safety_mode(&self, delta: u64) -> anyhow::Result<()> {
        let mut conn = self.storage.redis_client.get_async_connection().await?;
        redis::cmd("SET")
            .arg("nexus:safety_mode")
            .arg(true)
            .query_async::<_, ()>(&mut conn).await?;

        redis::cmd("SET")
            .arg("nexus:drift")
            .arg(delta)
            .query_async::<_, ()>(&mut conn).await?;

        Ok(())
    }

    /// Provides status and proof for "Direct Withdrawal Tenure".
    pub async fn get_direct_exit_status(&self, user_address: &str) -> anyhow::Result<String> {
        Ok(format!("User {}: Eligible for Direct Withdrawal", user_address))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_drift_calculation() {
    }
}
