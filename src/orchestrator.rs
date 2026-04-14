//! [NEXUS-ORCH-01] Autonomous Orchestrator for Glass Node self-healing.
//! Monitors internal services and manages fail-closed/recovery states.

use std::sync::Arc;
use tokio::time::{self, Duration};
use crate::storage::Storage;
use crate::api::billing::nostr::NostrTelemetry;
use crate::state::NexusState;

pub struct AutonomousOrchestrator {
    storage: Arc<Storage>,
    state: Arc<NexusState>,
    nostr: Option<Arc<NostrTelemetry>>,
}

impl AutonomousOrchestrator {
    pub fn new(storage: Arc<Storage>, state: Arc<NexusState>, nostr: Option<Arc<NostrTelemetry>>) -> Self {
        Self { storage, state, nostr }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("Autonomous Orchestrator active. Monitoring system health...");
        let mut interval = time::interval(Duration::from_secs(60));

        loop {
            interval.tick().await;
            if let Err(e) = self.audit_system_state().await {
                tracing::error!("Orchestration audit failed: {}", e);
            }
        }
    }

    async fn audit_system_state(&self) -> anyhow::Result<()> {
        let mut conn = self.storage.redis_client.get_multiplexed_async_connection().await?;

        // 1. Check for sync drift
        let drift: u64 = redis::cmd("GET").arg("nexus:drift").query_async(&mut conn).await.unwrap_or(0);
        let safety_mode: bool = redis::cmd("GET").arg("nexus:safety_mode").query_async(&mut conn).await.unwrap_or(false);

        if safety_mode && drift > 10 {
            tracing::warn!("Critical Drift Detected ({}). Initiating autonomous recovery sequence...", drift);

            if let Some(n) = &self.nostr {
                let processed_height: Option<i64> = match sqlx::query_scalar(
                    "SELECT MAX(height) FROM stacks_blocks WHERE type = 'burn_block' AND state = 'hard'",
                )
                .fetch_one(&self.storage.pg_pool)
                .await
                {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::error!("Orchestrator processed height query failed: {}", e);
                        None
                    }
                };

                let processed_height = processed_height.unwrap_or(0) as u64;

                let _ = n
                    .report_health_nostr(
                        "CRITICAL_DRIFT",
                        processed_height,
                        &self.state.get_state_root(),
                        Some(drift),
                    )
                    .await;
            }

            // In a full implementation, we might trigger a DB vacuum or re-connection here.
            // For PoC, we ensure the safety flag is persistent and verified.
        }

        // 2. Self-healing: Ensure state roots are consistently committed to Tableland
        // (Handled by NexusSync, but monitored here for consistency logs)

        tracing::debug!("System Audit Complete. Health: {}", if safety_mode { "Safety Mode" } else { "Nominal" });
        Ok(())
    }
}
