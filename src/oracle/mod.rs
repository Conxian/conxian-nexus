use crate::storage::Storage;
use crate::oracle::aggregator::{OracleAggregator, PppState};
use std::sync::Arc;
use tokio::time::{self, Duration};

pub mod aggregator;

pub struct OracleService {
    storage: Arc<Storage>,
    aggregator: OracleAggregator,
}

impl OracleService {
    pub fn new(storage: Arc<Storage>, endpoint_url: String, contract_principal: String) -> Self {
        Self {
            storage,
            aggregator: OracleAggregator::new(endpoint_url, contract_principal),
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("Starting OracleService...");
        let mut interval = time::interval(Duration::from_secs(60));

        loop {
            interval.tick().await;
            match self.aggregator.fetch_universal_fx().await {
                Ok(state) => {
                    if let Err(e) = self.persist_fx_state(&state).await {
                        tracing::error!("Failed to persist FX state: {}", e);
                    }
                    // Pushing to contract is optional/best-effort in the loop
                    let _ = self.aggregator.push_state_to_contract(state).await;
                }
                Err(e) => tracing::error!("Oracle fetch failed: {}", e),
            }
        }
    }

    async fn persist_fx_state(&self, state: &PppState) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO oracle_fx_history (base_currency, rates, ppp_indices, confidence_intervals, timestamp) VALUES ($1, $2, $3, $4, $5)")
            .bind(&state.base_currency)
            .bind(serde_json::to_value(&state.rates)?)
            .bind(serde_json::to_value(&state.ppp_indices)?)
            .bind(serde_json::to_value(&state.confidence_intervals)?)
            .bind(state.timestamp as i64)
            .execute(&self.storage.pg_pool)
            .await?;
        Ok(())
    }

    pub async fn verify_external_signal(&self, source: &str, payload: &serde_json::Value) -> anyhow::Result<bool> {
        // [CON-162] Verify external signal against Oracle state
        let rates = self.aggregator.fetch_universal_fx().await
            .map_err(|e| anyhow::anyhow!("Oracle fetch error: {}", e))?;

        if source == "ISO20022" {
            if let Some(payload_rate) = payload.get("exchange_rate").and_then(|v| v.as_f64()) {
                let currency = payload.get("currency").and_then(|v| v.as_str()).unwrap_or("USD");
                if let Some(oracle_rate) = rates.rates.get(currency) {
                    let diff = (payload_rate - oracle_rate).abs() / oracle_rate;
                    if diff > 0.05 { // 5% tolerance
                        tracing::warn!("Oracle verification failed for ISO20022: rate diff too high ({:.2}%)", diff * 100.0);
                        return Ok(false);
                    }
                }
            }
        }

        Ok(true)
    }
}
