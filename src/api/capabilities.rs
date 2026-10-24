//! Registered observation components. Registration does not establish live support.
use crate::api::rest::AppState;
use axum::{extract::State, http::header, Json};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Serialize, Deserialize)]
pub struct ObservationAdapterRegistration {
    pub adapter_id: String,
    pub runtime_state: String,
    pub health: String,
    pub production_support: String,
    pub qualified_operations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ObservationDiscoveryManifest {
    pub schema_version: u32,
    pub manifest_kind: String,
    pub repository: String,
    pub trust_root: String,
    pub scope: String,
    pub native_conxian_token: bool,
    pub adapters: Vec<ObservationAdapterRegistration>,
}

fn registration<T>(adapter_id: &str, _component: &T) -> ObservationAdapterRegistration {
    ObservationAdapterRegistration {
        adapter_id: adapter_id.into(),
        runtime_state: "registered".into(),
        health: "not-observed".into(),
        production_support: "not-evidenced".into(),
        qualified_operations: Vec::new(),
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
}
