//! Nexus proof and risk verification boundary.
//!
//! Thin, deterministic wrappers over `lib_conxian_core` primitives that anchor
//! Nexus's observation and proof responsibilities to the canonical Conxian
//! control model. Nexus observes and proves — it does not execute — so this
//! module intentionally reuses Core's platform-neutral contracts rather than
//! re-deriving chain identity, risk classification, or proof-envelope rules.

pub mod zkcp;

pub use zkcp::{ZkcpError, ZkcpProofPayload, ZkcpVerifier, ZKCP_CIRCUIT_ID};


use lib_conxian_core::control_model::{
    canonical_risk_profile_set, chain_family_for, Chain, ChainFamily, OverallRiskStatus,
    RiskProfile, RiskProfileError, RiskTarget,
};
use lib_conxian_core::verifier::ChainId;

/// Parse a wire-format chain-family token (e.g. `"cosmos_ibc"`) into the
/// canonical [`ChainFamily`]. Returns `None` for unknown or empty tokens.
///
/// Parsing is case-insensitive on the first fallback so that mixed-case wire
/// tokens such as `"EVM"` still resolve to [`ChainFamily::Evm`].
pub fn parse_chain_family(value: &str) -> Option<ChainFamily> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    parse_chain_family_token(value)
        .or_else(|| parse_chain_family_token(&value.to_ascii_lowercase()))
}

fn parse_chain_family_token(token: &str) -> Option<ChainFamily> {
    serde_json::from_value(serde_json::Value::String(token.to_string())).ok()
}

/// Build a canonical chain identifier for a known [`Chain`] and network.
pub fn chain_id(chain: Chain, network: impl Into<String>) -> ChainId {
    ChainId::from_chain(chain, network)
}

/// Look up the canonical, versioned risk profile for a specific [`Chain`].
pub fn risk_profile_for_chain(
    chain: Chain,
) -> Result<Option<&'static RiskProfile>, RiskProfileError> {
    let family = chain_family_for(&chain);
    let target = RiskTarget::Chain { chain, family };
    Ok(canonical_risk_profile_set()?.profile_for_target(&target))
}

/// Look up the canonical, versioned risk profile for a [`ChainFamily`].
pub fn risk_profile_for_family(
    family: ChainFamily,
) -> Result<Option<&'static RiskProfile>, RiskProfileError> {
    let target = RiskTarget::Family { family };
    Ok(canonical_risk_profile_set()?.profile_for_target(&target))
}

/// Return the assessed risk-band label for a profile, if one is present.
pub fn risk_band(profile: &RiskProfile) -> Option<&str> {
    match &profile.assessment.overall {
        OverallRiskStatus::Assessed { band } => Some(band.as_str()),
        OverallRiskStatus::Unknown { .. } | OverallRiskStatus::NotAssessed => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_chain_families() {
        assert_eq!(
            parse_chain_family("cosmos_ibc"),
            Some(ChainFamily::CosmosIbc)
        );
        assert_eq!(parse_chain_family("evm"), Some(ChainFamily::Evm));
        assert_eq!(
            parse_chain_family("bitcoin_utxo"),
            Some(ChainFamily::BitcoinUtxo)
        );
        assert_eq!(parse_chain_family("anchor"), Some(ChainFamily::Anchor));
        assert_eq!(
            parse_chain_family("solana_svm"),
            Some(ChainFamily::SolanaSvm)
        );
        assert_eq!(parse_chain_family("EVM"), Some(ChainFamily::Evm));
    }

    #[test]
    fn rejects_unknown_or_empty_families() {
        assert_eq!(parse_chain_family("bogus"), None);
        assert_eq!(parse_chain_family(""), None);
        assert_eq!(parse_chain_family("   "), None);
    }

    #[test]
    fn builds_canonical_chain_id() {
        let id = chain_id(Chain::Bitcoin, "mainnet");
        assert_eq!(id.family, ChainFamily::BitcoinUtxo);
        assert!(id.validate().is_ok());
        assert!(!id.canonical_id().is_empty());
    }

    #[test]
    fn resolves_canonical_risk_profile_for_chain() {
        let profile = risk_profile_for_chain(Chain::Bitcoin)
            .expect("canonical risk profile set loads")
            .expect("bitcoin has a canonical profile");
        assert_eq!(profile.target.family(), ChainFamily::BitcoinUtxo);
        assert!(!profile.rationale.is_empty());
    }

    #[test]
    fn resolves_canonical_risk_profile_for_family() {
        let profile = risk_profile_for_family(ChainFamily::Evm)
            .expect("canonical risk profile set loads")
            .expect("evm family has a canonical profile");
        assert_eq!(profile.target.family(), ChainFamily::Evm);
    }
}
