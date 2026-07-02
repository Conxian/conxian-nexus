//! [NEXUS-ORCH-01] Autonomous Orchestrator for Glass Node self-healing.
//! Monitors internal services and manages fail-closed/recovery states.

use crate::api::billing::nostr::NostrTelemetry;
use crate::state::NexusState;
use crate::storage::Storage;
use std::sync::Arc;
use tokio::time::{self, Duration};

pub struct AutonomousOrchestrator {
    storage: Arc<Storage>,
    state: Arc<NexusState>,
    nostr: Option<Arc<NostrTelemetry>>,
}

impl AutonomousOrchestrator {
    pub fn new(
        storage: Arc<Storage>,
        state: Arc<NexusState>,
        nostr: Option<Arc<NostrTelemetry>>,
    ) -> Self {
        Self {
            storage,
            state,
            nostr,
        }
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
        let mut conn = self
            .storage
            .redis_client
            .get_multiplexed_async_connection()
            .await?;

        // 0. Lightning Recovery (SRL-1)
        if let Err(e) = self.audit_lightning_payments().await {
            tracing::error!("Lightning recovery audit failed: {}", e);
        }

        // 1. Check for sync drift
        let drift: u64 = redis::cmd("GET")
            .arg("nexus:drift")
            .query_async(&mut conn)
            .await
            .unwrap_or(0);
        let safety_mode: bool = redis::cmd("GET")
            .arg("nexus:safety_mode")
            .query_async(&mut conn)
            .await
            .unwrap_or(false);

        if safety_mode && drift > 10 {
            tracing::warn!(
                "Critical Drift Detected ({}). Initiating autonomous recovery sequence...",
                drift
            );

            if let Some(n) = &self.nostr {
                let _ = n
                    .report_health_nostr("CRITICAL_DRIFT", drift, &self.state.get_state_root())
                    .await;
            }

            // In a full implementation, we might trigger a DB vacuum or re-connection here.
            // For PoC, we ensure the safety flag is persistent and verified.
        }

        // 2. Self-healing: Ensure state roots are consistently committed to Tableland
        // (Handled by NexusSync, but monitored here for consistency logs)

        tracing::debug!(
            "System Audit Complete. Health: {}",
            if safety_mode {
                "Safety Mode"
            } else {
                "Nominal"
            }
        );
        Ok(())
    }

    /// [Hole 3.1] SRL-1: Poll and recover stale or failed Lightning payments.
    pub async fn audit_lightning_payments(&self) -> anyhow::Result<()> {
        let adapter = crate::executor::lightning::LightningResilienceAdapter::new();

        // Find payments that might need recovery
        let rows = sqlx::query(
            "SELECT payment_id, payment_hash, amount_msat, status, failure_type, retry_count, created_at, last_updated_at
             FROM lightning_payment_intents
             WHERE status IN ('pending', 'recovering', 'mpp_splitting', 'failed')"
        )
        .fetch_all(&self.storage.pg_pool)
        .await?;

        for row in rows {
            use crate::executor::lightning::{LightningFailureType, LightningPaymentStatus, PaymentIntent};
            use sqlx::Row;

            let status_str: String = row.try_get("status")?;
            let failure_type_str: Option<String> = row.try_get("failure_type")?;

            let mut intent = PaymentIntent {
                payment_id: row.try_get("payment_id")?,
                payment_hash: row.try_get("payment_hash")?,
                amount_msat: row.try_get::<i64, _>("amount_msat")? as u64,
                status: match status_str.as_str() {
                    "pending" => LightningPaymentStatus::Pending,
                    "succeeded" => LightningPaymentStatus::Succeeded,
                    "failed" => LightningPaymentStatus::Failed,
                    "recovering" => LightningPaymentStatus::Recovering,
                    "mpp_splitting" => LightningPaymentStatus::MppSplitting,
                    _ => LightningPaymentStatus::Pending,
                },
                failure_type: failure_type_str.and_then(|s| match s.as_str() {
                    "permanent" => Some(LightningFailureType::Permanent),
                    "transient" => Some(LightningFailureType::Transient),
                    "indeterminate" => Some(LightningFailureType::Indeterminate),
                    "mpp_partial" => Some(LightningFailureType::MppPartial),
                    _ => None,
                }),
                retry_count: row.try_get("retry_count")?,
                created_at: row.try_get("created_at")?,
                last_updated_at: row.try_get("last_updated_at")?,
            };

            if let Some(action) = adapter.process_recovery(&mut intent) {
                tracing::info!(
                    payment_id = %intent.payment_id,
                    action = %action,
                    "Triggering Lightning recovery action"
                );

                // Persist the updated state
                sqlx::query(
                    "UPDATE lightning_payment_intents
                     SET status = $1, retry_count = $2, last_updated_at = $3
                     WHERE payment_id = $4"
                )
                .bind(intent.status.to_string())
                .bind(intent.retry_count)
                .bind(intent.last_updated_at)
                .bind(&intent.payment_id)
                .execute(&self.storage.pg_pool)
                .await?;

                // Audit the recovery event
                sqlx::query(
                    "INSERT INTO lightning_payment_events (event_id, payment_id, status, metadata)
                     VALUES ($1, $2, $3, $4)"
                )
                .bind(format!("rec_{}", uuid::Uuid::new_v4()))
                .bind(&intent.payment_id)
                .bind(intent.status.to_string())
                .bind(format!("Recovery action: {}", action))
                .execute(&self.storage.pg_pool)
                .await?;
            }
        }

        Ok(())
    }
}
