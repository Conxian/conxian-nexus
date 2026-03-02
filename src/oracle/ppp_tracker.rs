use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The structure of the universal fiat state we will push on-chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PppState {
    pub base_currency: String,
    pub rates: HashMap<String, f64>,
    pub ppp_indices: HashMap<String, f64>,
    pub timestamp: u64,
}

#[allow(dead_code)]
pub struct OracleStub {
    client: Client,
    endpoint_url: String, 
}

impl OracleStub {
    pub fn new(endpoint_url: String) -> Self {
        Self {
            client: Client::new(),
            endpoint_url,
        }
    }

    /// Fetches the latest global FX rates for dynamic PPP pricing
    pub async fn fetch_universal_fx(&self) -> Result<PppState, Box<dyn std::error::Error + Send + Sync>> {
        // Stub implementation for development
        let mut mock_rates = HashMap::new();
        mock_rates.insert("ZAR".to_string(), 18.5);
        mock_rates.insert("NGN".to_string(), 1500.0);
        mock_rates.insert("BRL".to_string(), 5.0);
        mock_rates.insert("EUR".to_string(), 0.92);
        
        let mut mock_ppp = HashMap::new();
        mock_ppp.insert("ZAR".to_string(), 0.45); 
        mock_ppp.insert("NGN".to_string(), 0.30);
        mock_ppp.insert("BRL".to_string(), 0.50);
        mock_ppp.insert("EUR".to_string(), 1.0);

        Ok(PppState {
            base_currency: "USD".to_string(),
            rates: mock_rates,
            ppp_indices: mock_ppp,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }

    /// Propagates the fetched universal FX state to the Stacks Testnet contract
    pub async fn push_state_to_contract(&self, state: PppState) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("Pushing Oracle State on-chain: {:?}", state);
        Ok("mock_tx_id_0x123abc".to_string())
    }
}
