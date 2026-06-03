use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

/// Rollout modes for the RGB Protocol Adapter.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RGBRolloutMode {
    /// Adapter is inactive and will reject all requests.
    Disabled,
    /// Adapter processes requests and logs behavior without production side-effects.
    Shadow,
    /// Adapter is fully active.
    Active,
}

impl fmt::Display for RGBRolloutMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Disabled => "disabled",
            Self::Shadow => "shadow",
            Self::Active => "active",
        };
        write!(f, "{}", s)
    }
}

/// Protocol Adapter for RGB (Really Good Bitcoin) smart contracts.
pub struct RGBAdapter {
    pub mode: RGBRolloutMode,
    pub known_contracts: HashSet<String>,
}

impl RGBAdapter {
    /// Creates a new RGBAdapter with the specified rollout mode.
    pub fn new(mode: RGBRolloutMode) -> Self {
        Self {
            mode,
            known_contracts: HashSet::new(),
        }
    }

    /// Creates a new RGBAdapter with known contracts.
    pub fn with_known_contracts(mode: RGBRolloutMode, known: HashSet<String>) -> Self {
        Self {
            mode,
            known_contracts: known,
        }
    }

    /// Performs a contract lookup.
    ///
    /// In 'shadow' and 'active' modes, this currently returns a verified mock payload
    /// for contract IDs starting with "rgb:".
    pub async fn lookup_contract(&self, contract_id: &str) -> anyhow::Result<Option<String>> {
        if !contract_id.starts_with("rgb:") || contract_id.len() < 10 {
             anyhow::bail!("Invalid RGB contract ID format: must start with rgb: and have sufficient length");
        }

        match self.mode {
            RGBRolloutMode::Disabled => {
                anyhow::bail!("RGB adapter is disabled");
            }
            RGBRolloutMode::Shadow => {
                tracing::info!("[SHADOW] RGB contract lookup for: {}", contract_id);
                let mock_payload = format!(
                    "{{\"contract_id\": \"{}\", \"status\": \"verified\", \"mode\": \"shadow\"}}",
                    contract_id
                );
                Ok(Some(mock_payload))
            }
            RGBRolloutMode::Active => {
                // TODO: Wire to node-backed data for real contract lookup.
                if self.known_contracts.contains(contract_id) {
                     let payload = format!(
                        "{{\"contract_id\": \"{}\", \"status\": \"active\", \"mode\": \"active\"}}",
                        contract_id
                    );
                    Ok(Some(payload))
                } else {
                    tracing::warn!("RGB contract not found in known set (Active Mode): {}", contract_id);
                    Ok(None)
                }
            }
        }
    }
}
