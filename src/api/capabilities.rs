//! Registered observation components. Registration does not establish live support.
use crate::api::rest::AppState;
use axum::{extract::State, http::header, Json};
use lib_conxian_core::capability::CHAIN_TAXONOMY_VERSION;
use lib_conxian_core::control_model::{chain_family_for, Chain, ALL_CHAINS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Render an enum's `snake_case` serde name (the wire identity), if any.
fn snake(value: &impl serde::Serialize) -> Option<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
}

/// Resolve a runtime adapter id to a Core taxonomy chain, when one exists.
fn chain_for_adapter_id(adapter_id: &str) -> Option<Chain> {
    ALL_CHAINS
        .iter()
        .find(|chain| snake(chain).as_deref() == Some(adapter_id))
        .cloned()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ObservationAdapterRegistration {
    pub adapter_id: String,
    pub runtime_state: String,
    pub health: String,
    pub production_support: String,
    pub qualified_operations: Vec<String>,
    /// Operation classes this observer supports. Nexus observes chains; adapters
    /// with no Core taxonomy binding are reported `unavailable` rather than
    /// inferred observable (Nexus #418).
    pub operation_classes: Vec<String>,
    /// Bound Core taxonomy identity, when the adapter id maps to a known chain.
    pub chain: Option<String>,
    pub family: Option<String>,
    /// Structural evidence; registration is not a support claim.
    pub evidence: CapabilityEvidence,
    /// Nexus observes and verifies; it never takes custody of user funds.
    pub no_custody: bool,
}

/// Structural evidence for one advertised observer. Every field is
/// `not_evidenced` unless a producer has recorded independent evidence.
#[derive(Debug, Serialize, Deserialize)]
pub struct CapabilityEvidence {
    pub api: String,
    pub implementation: String,
    pub integration: String,
    pub independent_review: String,
    pub production_support: String,
    pub references: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ObservationDiscoveryManifest {
    pub schema_version: u32,
    pub manifest_kind: String,
    pub repository: String,
    pub trust_root: String,
    pub scope: String,
    pub native_conxian_token: bool,
    /// Version of the Core chain taxonomy this manifest is bound to.
    pub taxonomy_version: String,
    pub adapters: Vec<ObservationAdapterRegistration>,
}

/// Classify an adapter's operation classes without inferring live support.
fn classify(adapter_id: &str) -> (Vec<String>, Option<String>, Option<String>) {
    match chain_for_adapter_id(adapter_id) {
        Some(chain) => {
            let family = chain_family_for(&chain);
            (
                vec!["observe".to_string(), "verify".to_string()],
                snake(&chain),
                snake(&family),
            )
        }
        None => (vec!["unavailable".to_string()], None, None),
    }
}

/// Unclaimed structural evidence. Registration and a mounted route are not
/// evidence, so every producer fact stays `not_evidenced`.
fn unclaimed_evidence() -> CapabilityEvidence {
    CapabilityEvidence {
        api: "not_evidenced".into(),
        implementation: "not_evidenced".into(),
        integration: "not_evidenced".into(),
        independent_review: "not_evidenced".into(),
        production_support: "not_evidenced".into(),
        references: vec!["GET /api/capabilities".into()],
    }
}

fn registration<T>(adapter_id: &str, _component: &T) -> ObservationAdapterRegistration {
    let (operation_classes, chain, family) = classify(adapter_id);
    ObservationAdapterRegistration {
        adapter_id: adapter_id.into(),
        runtime_state: "registered".into(),
        health: "not-observed".into(),
        production_support: "not-evidenced".into(),
        qualified_operations: Vec::new(),
        operation_classes,
        chain,
        family,
        evidence: unclaimed_evidence(),
        no_custody: true,
    }
}

pub fn registered_observation_adapters(state: &AppState) -> ObservationDiscoveryManifest {
    // These references bind discovery to actual typed components in AppState.
    // None of their RPC/provider state is probed or inferred here.
    let adapters = vec![
        registration("bitcoin", &state.nexus_state),
        registration("aptos", &state.executor.aptos_adapter),
        registration("cosmos", &state.executor.cosmos_adapter),
        registration("evm", &state.executor.evm_adapter),
        registration("fedimint", &state.executor.fedimint_adapter),
        registration("lightning", &state.executor.lightning_adapter),
        registration("rgb", &state.executor.rgb_adapter),
        registration("solana", &state.executor.solana_adapter),
        registration("stacks", &state.executor.stacks_adapter),
        registration("sui", &state.executor.sui_adapter),
    ];
    observation_discovery(adapters)
}

fn observation_discovery(
    mut adapters: Vec<ObservationAdapterRegistration>,
) -> ObservationDiscoveryManifest {
    let mut seen = BTreeSet::new();
    adapters.retain(|a| seen.insert(a.adapter_id.clone()));
    adapters.sort_by(|a, b| a.adapter_id.cmp(&b.adapter_id));
    ObservationDiscoveryManifest {
        schema_version: 1,
        manifest_kind: "adapter_registration".into(),
        repository: "Conxian/conxian-nexus".into(),
        trust_root: "bitcoin-l1".into(),
        scope: "universal-chain".into(),
        native_conxian_token: false,
        taxonomy_version: CHAIN_TAXONOMY_VERSION.into(),
        adapters,
    }
}

pub async fn get_capabilities(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(registered_observation_adapters(&state)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrations_are_sorted_without_qualified_operations() {
        let manifest = observation_discovery(vec![
            registration("evm", &()),
            registration("bitcoin", &()),
            registration("evm", &()),
        ]);
        assert_eq!(
            manifest
                .adapters
                .iter()
                .map(|a| a.adapter_id.as_str())
                .collect::<Vec<_>>(),
            ["bitcoin", "evm"]
        );
        assert!(manifest.adapters.iter().all(|a| a.health == "not-observed"
            && a.production_support == "not-evidenced"
            && a.qualified_operations.is_empty()));
    }

    #[test]
    fn empty_discovery_does_not_infer_sdk_connections() {
        assert!(observation_discovery(Vec::new()).adapters.is_empty());
    }

    #[test]
    fn manifest_is_bound_to_the_core_taxonomy_version() {
        let manifest = observation_discovery(Vec::new());
        assert_eq!(manifest.taxonomy_version, CHAIN_TAXONOMY_VERSION);
    }

    #[test]
    fn known_chain_adapter_is_bound_to_taxonomy() {
        let manifest = observation_discovery(vec![registration("bitcoin", &())]);
        let bitcoin = &manifest.adapters[0];
        assert_eq!(bitcoin.chain.as_deref(), Some("bitcoin"));
        assert!(bitcoin.family.is_some());
        assert_eq!(bitcoin.operation_classes, ["observe", "verify"]);
    }

    #[test]
    fn registration_never_claims_evidence_or_custody() {
        let manifest = observation_discovery(vec![registration("bitcoin", &())]);
        let adapter = &manifest.adapters[0];
        assert_eq!(adapter.evidence.api, "not_evidenced");
        assert_eq!(adapter.evidence.production_support, "not_evidenced");
        assert!(adapter.no_custody);
    }
}
