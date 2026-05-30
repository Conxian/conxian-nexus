use serde::{Deserialize, Serialize};
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
}

impl RGBAdapter {
    /// Creates a new RGBAdapter with the specified rollout mode.
    pub fn new(mode: RGBRolloutMode) -> Self {
        Self { mode }
    }

    /// Performs a contract lookup.
    ///
    /// In 'shadow' and 'active' modes, this currently returns a verified mock payload
    /// for contract IDs starting with "rgb:".
    pub async fn lookup_contract(&self, contract_id: &str) -> anyhow::Result<Option<String>> {
        match self.mode {
            RGBRolloutMode::Disabled => {
                anyhow::bail!("RGB adapter is disabled");
            }
            RGBRolloutMode::Shadow | RGBRolloutMode::Active => {
                // Milestone 2: Shadow-Mode Adapter PoC
                // TODO: Wire to node-backed data for real contract lookup.
                if contract_id.starts_with("rgb:") {
                    let mock_payload = format!(
                        "{{\"contract_id\": \"{}\", \"status\": \"verified\", \"mode\": \"{}\"}}",
                        contract_id, self.mode
                    );

                    if self.mode == RGBRolloutMode::Shadow {
                        tracing::info!("[SHADOW] RGB contract lookup for: {}", contract_id);
                    }

                    Ok(Some(mock_payload))
                } else {
                    Ok(None)
                }
            }
        }
    }
}
