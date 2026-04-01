pub mod ppp_tracker;

use crate::oracle::ppp_tracker::{OracleStub, PppState};
use crate::storage::Storage;
use std::sync::Arc;
use tokio::time::{self, Duration};

pub struct OracleService {
    pub storage: Arc<Storage>,
    pub stub: OracleStub,
}

impl OracleService {
    pub fn new(storage: Arc<Storage>, endpoint_url: String) -> Self {
        Self {
            storage,
            stub: OracleStub::new(endpoint_url),
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let mut interval = time::interval(Duration::from_secs(60));
        tracing::info!("Starting OracleService...");

        loop {
            interval.tick().await;
            match self.stub.fetch_universal_fx().await {
                Ok(state) => {
                    let tx_id = match self.stub.push_state_to_contract(state.clone()).await {
                        Ok(id) => Some(id),
                        Err(e) => {
                            tracing::error!("Failed to push oracle state on-chain: {}", e);
                            None
                        }
                    };

                    if let Err(e) = self.persist_fx_history(state, tx_id).await {
                        tracing::error!("Failed to persist FX history: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to fetch FX rates: {}", e);
                }
            }
        }
    }

    async fn persist_fx_history(
        &self,
        state: PppState,
        tx_id: Option<String>,
    ) -> anyhow::Result<()> {
        let rates_json = serde_json::to_value(&state.rates)?;
        let ppp_json = serde_json::to_value(&state.ppp_indices)?;

        sqlx::query(
            "INSERT INTO oracle_fx_history (base_currency, rates, ppp_indices, timestamp, tx_id) VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(&state.base_currency)
        .bind(rates_json)
        .bind(ppp_json)
        .bind(state.timestamp as i64)
        .bind(tx_id)
        .execute(&self.storage.pg_pool).await?;

        Ok(())
    }
}
