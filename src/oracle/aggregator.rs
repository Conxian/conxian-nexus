use lib_conxian_core::{ContractBridge, Wallet};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

pub struct OracleAggregator {
    client: Client,
    endpoint_urls: Vec<String>,
    contract_principal: String,
}

impl OracleAggregator {
    pub fn new(endpoint_url: String, contract_principal: String) -> Self {
        Self {
            client: Client::new(),
            endpoint_urls: vec![
                endpoint_url,
                "https://open.er-api.com/v6/latest/USD".to_string(),
                "https://api.exchangerate.host/latest?base=USD".to_string(),
            ],
            contract_principal,
        }
    }

    pub async fn fetch_universal_fx(
        &self,
    ) -> Result<PppState, Box<dyn std::error::Error + Send + Sync>> {
        let mut all_rates: Vec<HashMap<String, f64>> = Vec::new();

        for url in &self.endpoint_urls {
            match self.client.get(url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        if let Ok(data) = resp.json::<ExchangeRateResponse>().await {
                            all_rates.push(data.rates);
                        }
                    }
                }
                Err(e) => tracing::warn!("Failed to fetch from {}: {}", url, e),
            }
        }

        if all_rates.is_empty() {
            tracing::error!("All Oracle endpoints failed. No rates available.");
            return Err("Oracle failure".into());
        }

        let mut aggregated_rates = HashMap::new();
        let mut keys: std::collections::HashSet<String> = all_rates[0].keys().cloned().collect();
        for r in &all_rates[1..] {
            keys.extend(r.keys().cloned());
        }

        for key in keys {
            let mut values: Vec<f64> = all_rates
                .iter()
                .filter_map(|r| r.get(&key).copied())
                .collect();

            if !values.is_empty() {
                values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let median = values[values.len() / 2];
                aggregated_rates.insert(key, median);
            }
        }

        // Production mapping for regional currencies
        aggregated_rates.entry("ZAR".to_string()).or_insert(18.5);
        aggregated_rates.entry("NGN".to_string()).or_insert(1500.0);
        aggregated_rates.entry("BRL".to_string()).or_insert(5.0);

        let mut ppp_indices = HashMap::new();
        ppp_indices.insert("ZAR".to_string(), 0.45);
        ppp_indices.insert("NGN".to_string(), 0.30);
        ppp_indices.insert("BRL".to_string(), 0.50);
        ppp_indices.insert("EUR".to_string(), 1.0);

        Ok(PppState {
            base_currency: "USD".to_string(),
            rates: aggregated_rates,
            ppp_indices,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| anyhow::anyhow!("Time failure: {}", e))?
                .as_secs(),
        })
    }

    pub async fn push_state_to_contract(
        &self,
        state: PppState,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let wallet = Wallet::new().map_err(|e| anyhow::anyhow!("Wallet creation failed: {}", e))?;
        let state_json = serde_json::to_string(&state)
            .map_err(|e| anyhow::anyhow!("State serialization failed: {}", e))?;

        let signed_call = ContractBridge::create_signed_call(
            &wallet,
            &self.contract_principal,
            "update-fx-rates",
            vec![state_json],
        ).map_err(|e| anyhow::anyhow!("Contract call signing failed: {}", e))?;

        tracing::info!("Pushing Signed Oracle Call: {:?}", signed_call.payload);
        Ok(format!("0x{}", signed_call.signature))
    }
}
