use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use lib_conxian_core::{Wallet, ContractBridge};

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

    pub async fn fetch_universal_fx(&self) -> Result<PppState, Box<dyn std::error::Error + Send + Sync>> {
        let resp = self.client.get(&self.endpoint_url).send().await?;
        
        let mut rates = if resp.status().is_success() {
            let data: ExchangeRateResponse = resp.json().await?;
            data.rates
        } else {
            let mut fallback = HashMap::new();
            fallback.insert("EUR".to_string(), 0.92);
            fallback
        };

        rates.entry("ZAR".to_string()).or_insert(18.5);
        rates.entry("NGN".to_string()).or_insert(1500.0);
        rates.entry("BRL".to_string()).or_insert(5.0);

        let mut ppp_indices = HashMap::new();
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

    pub async fn push_state_to_contract(&self, state: PppState) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let wallet = Wallet::new();
        let state_json = serde_json::to_string(&state).unwrap_or_default();

        let signed_call = ContractBridge::create_signed_call(
            &wallet,
            "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.oracle-v1",
            "update-fx-rates",
            vec![state_json]
        );

        tracing::info!("Pushing Signed Oracle Call: {:?}", signed_call.payload);
        Ok(format!("0x{}", signed_call.signature))
    }
}
