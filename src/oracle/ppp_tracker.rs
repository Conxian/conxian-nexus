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

#[derive(Deserialize)]
struct ExchangeRateResponse {
    rates: HashMap<String, f64>,
}

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
        let resp = self.client.get(&self.endpoint_url).send().await?;
        
        let mut rates = if resp.status().is_success() {
            let data: ExchangeRateResponse = resp.json().await?;
            data.rates
        } else {
            tracing::warn!("Failed to fetch FX rates from {}, using fallback", self.endpoint_url);
            let mut fallback = HashMap::new();
            fallback.insert("EUR".to_string(), 0.92);
            fallback
        };

        // Ensure we have some base rates if the API fails or returns partial data
        rates.entry("ZAR".to_string()).or_insert(18.5);
        rates.entry("NGN".to_string()).or_insert(1500.0);
        rates.entry("BRL".to_string()).or_insert(5.0);

        let mut ppp_indices = HashMap::new();
        // Mock PPP adjustment ratios (in a real scenario, these would be fetched from a World Bank/IMF API)
        ppp_indices.insert("ZAR".to_string(), 0.45);
        ppp_indices.insert("NGN".to_string(), 0.30);
        ppp_indices.insert("BRL".to_string(), 0.50);
        ppp_indices.insert("EUR".to_string(), 1.0);

        Ok(PppState {
            base_currency: "USD".to_string(),
            rates,
            ppp_indices,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }

    /// Propagates the fetched universal FX state to the Stacks Testnet contract
    pub async fn push_state_to_contract(&self, state: PppState) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("Pushing Oracle State on-chain: {:?}", state);
        // In production, this would use lib-conxian-core to sign and broadcast a Clarity contract call
        Ok("mock_tx_id_0x123abc".to_string())
    }
}
