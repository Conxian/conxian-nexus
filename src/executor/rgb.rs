use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

/// Supported RGB Schemas.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RGBSchema {
    /// Non-Inflatable Assets (NIA)
    NIA,
    /// LNPBP (Lightning Network Protocol / Bitcoin Protocol)
    LNPBP,
    /// Unknown or generic schema
    Unknown,
}

impl fmt::Display for RGBSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NIA => write!(f, "NIA"),
            Self::LNPBP => write!(f, "LNPBP"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

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

/// Metadata for an RGB contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RGBContractMetadata {
    pub contract_id: String,
    pub status: String,
    pub mode: RGBRolloutMode,
    pub schema: RGBSchema,
    pub supply_total: Option<u64>,
    pub issued_at_height: Option<u64>,
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

    /// Validates if a contract ID matches expected format and schema rules.
    pub fn validate_contract_id(&self, contract_id: &str) -> anyhow::Result<RGBSchema> {
        if !contract_id.starts_with("rgb:") {
            anyhow::bail!("Invalid RGB contract ID prefix: expected 'rgb:'");
        }

        if contract_id.len() < 40 {
            anyhow::bail!("Invalid RGB contract ID length: too short");
        }

        // Schema heuristics based on suffix or patterns (simulated)
        if contract_id.contains("_nia") {
            Ok(RGBSchema::NIA)
        } else if contract_id.contains("_lnpbp") {
            Ok(RGBSchema::LNPBP)
        } else {
            Ok(RGBSchema::Unknown)
        }
    }

    /// Performs a contract lookup.
    pub async fn lookup_contract(
        &self,
        contract_id: &str,
    ) -> anyhow::Result<Option<RGBContractMetadata>> {
        let schema = self.validate_contract_id(contract_id)?;

        match self.mode {
            RGBRolloutMode::Disabled => {
                anyhow::bail!("RGB adapter is disabled");
            }
            RGBRolloutMode::Shadow => {
                tracing::info!(
                    "[SHADOW] RGB contract lookup for: {} (Schema: {})",
                    contract_id,
                    schema
                );
                Ok(Some(RGBContractMetadata {
                    contract_id: contract_id.to_string(),
                    status: "verified".to_string(),
                    mode: RGBRolloutMode::Shadow,
                    schema,
                    supply_total: Some(100_000_000),
                    issued_at_height: Some(840_000),
                }))
            }
            RGBRolloutMode::Active => {
                if self.known_contracts.contains(contract_id) {
                    Ok(Some(RGBContractMetadata {
                        contract_id: contract_id.to_string(),
                        status: "active".to_string(),
                        mode: RGBRolloutMode::Active,
                        schema,
                        supply_total: Some(100_000_000),
                        issued_at_height: Some(840_000),
                    }))
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
        "rgb:contract-123-long-enough-id-for-validation"
    }

    #[test]
    fn test_rollout_mode_display() {
        assert_eq!(RGBRolloutMode::Disabled.to_string(), "disabled");
        assert_eq!(RGBRolloutMode::Shadow.to_string(), "shadow");
        assert_eq!(RGBRolloutMode::Active.to_string(), "active");
    }

    #[test]
    fn test_schema_display() {
        assert_eq!(RGBSchema::NIA.to_string(), "NIA");
        assert_eq!(RGBSchema::LNPBP.to_string(), "LNPBP");
        assert_eq!(RGBSchema::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn test_schema_validation() {
        let adapter = RGBAdapter::new(RGBRolloutMode::Shadow);
        assert_eq!(
            adapter
                .validate_contract_id("rgb:asset_nia_123456789012345678901234567890")
                .unwrap(),
            RGBSchema::NIA
        );
        assert_eq!(
            adapter
                .validate_contract_id("rgb:asset_lnpbp_123456789012345678901234567890")
                .unwrap(),
            RGBSchema::LNPBP
        );
        assert_eq!(
            adapter.validate_contract_id(valid_contract_id()).unwrap(),
            RGBSchema::Unknown
        );

        let err_prefix = adapter.validate_contract_id("invalid").unwrap_err();
        assert!(err_prefix.to_string().contains("prefix"));

        let err_len = adapter.validate_contract_id("rgb:short").unwrap_err();
        assert!(err_len.to_string().contains("length"));
    }

    #[tokio::test]
    async fn test_lookup_contract_returns_verified_payload_in_shadow_mode() {
        let adapter = RGBAdapter::new(RGBRolloutMode::Shadow);
        let metadata = adapter
            .lookup_contract(valid_contract_id())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(metadata.contract_id, valid_contract_id());
        assert_eq!(metadata.status, "verified");
        assert_eq!(metadata.mode, RGBRolloutMode::Shadow);
        assert_eq!(metadata.schema, RGBSchema::Unknown);
    }

    #[tokio::test]
    async fn test_lookup_contract_disabled_bails() {
        let adapter = RGBAdapter::new(RGBRolloutMode::Disabled);
        let res = adapter.lookup_contract(valid_contract_id()).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("disabled"));
    }

    #[tokio::test]
    async fn test_lookup_contract_active_found() {
        let mut known = HashSet::new();
        let cid = valid_contract_id();
        known.insert(cid.to_string());
        let adapter = RGBAdapter::with_known_contracts(RGBRolloutMode::Active, known);

        let metadata = adapter.lookup_contract(cid).await.unwrap().unwrap();
        assert_eq!(metadata.status, "active");
        assert_eq!(metadata.mode, RGBRolloutMode::Active);
    }

    #[tokio::test]
    async fn test_lookup_contract_active_not_found() {
        let adapter = RGBAdapter::new(RGBRolloutMode::Active);
        let res = adapter.lookup_contract(valid_contract_id()).await.unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn test_adapter_constructors() {
        let adapter1 = RGBAdapter::new(RGBRolloutMode::Disabled);
        assert_eq!(adapter1.mode, RGBRolloutMode::Disabled);
        assert!(adapter1.known_contracts.is_empty());

        let mut known = HashSet::new();
        known.insert("test".to_string());
        let adapter2 = RGBAdapter::with_known_contracts(RGBRolloutMode::Active, known);
        assert_eq!(adapter2.mode, RGBRolloutMode::Active);
        assert_eq!(adapter2.known_contracts.len(), 1);
    }
}
