pub mod ppp_tracker;

use crate::storage::Storage;
use std::sync::Arc;
use tokio::time::{self, Duration};
use crate::oracle::ppp_tracker::OracleStub;

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
                    if let Err(e) = self.stub.push_state_to_contract(state).await {
                        tracing::error!("Failed to push oracle state on-chain: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to fetch FX rates: {}", e);
                }
            }
        }
    }
}
