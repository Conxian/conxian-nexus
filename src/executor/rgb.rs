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
            anyhow::bail!(
                "Invalid RGB contract ID format: must start with rgb: and have sufficient length"
            );
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
                    tracing::warn!(
                        "RGB contract not found in known set (Active Mode): {}",
                        contract_id
                    );
                    Ok(None)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_contract_id() -> &'static str {
        "rgb:contract-123"
    }

    #[test]
    fn test_rollout_mode_display() {
        assert_eq!(RGBRolloutMode::Disabled.to_string(), "disabled");
        assert_eq!(RGBRolloutMode::Shadow.to_string(), "shadow");
        assert_eq!(RGBRolloutMode::Active.to_string(), "active");
    }

    #[test]
    fn test_new_initializes_empty_known_contracts() {
        let adapter = RGBAdapter::new(RGBRolloutMode::Shadow);
        assert_eq!(adapter.mode, RGBRolloutMode::Shadow);
        assert!(adapter.known_contracts.is_empty());
    }

    #[tokio::test]
    async fn test_lookup_contract_rejects_invalid_contract_id() {
        let adapter = RGBAdapter::new(RGBRolloutMode::Active);
        let err = adapter.lookup_contract("invalid").await.unwrap_err();
        assert!(err
            .to_string()
            .contains("Invalid RGB contract ID format: must start with rgb:"));
    }

    #[tokio::test]
    async fn test_lookup_contract_rejects_when_adapter_disabled() {
        let adapter = RGBAdapter::new(RGBRolloutMode::Disabled);
        let err = adapter
            .lookup_contract(valid_contract_id())
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "RGB adapter is disabled");
    }

    #[tokio::test]
    async fn test_lookup_contract_returns_verified_payload_in_shadow_mode() {
        let adapter = RGBAdapter::new(RGBRolloutMode::Shadow);
        let payload = adapter
            .lookup_contract(valid_contract_id())
            .await
            .unwrap()
            .unwrap();

        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(json["contract_id"], valid_contract_id());
        assert_eq!(json["status"], "verified");
        assert_eq!(json["mode"], "shadow");
    }

    #[tokio::test]
    async fn test_lookup_contract_returns_active_payload_when_found() {
        let mut known = HashSet::new();
        known.insert(valid_contract_id().to_string());
        let adapter = RGBAdapter::with_known_contracts(RGBRolloutMode::Active, known);

        let payload = adapter
            .lookup_contract(valid_contract_id())
            .await
            .unwrap()
            .unwrap();

        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(json["contract_id"], valid_contract_id());
        assert_eq!(json["status"], "active");
        assert_eq!(json["mode"], "active");
    }

    #[tokio::test]
    async fn test_lookup_contract_returns_none_when_not_found_in_active_mode() {
        let adapter = RGBAdapter::with_known_contracts(RGBRolloutMode::Active, HashSet::new());
        assert_eq!(
            adapter.lookup_contract(valid_contract_id()).await.unwrap(),
            None
        );
    }
}
