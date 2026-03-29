//! [CON-66] Conxius Identity Service.
//! Implements plug-and-play resolution for ENS, BNS, and World ID.

use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use crate::api::rest::AppState;

#[derive(Debug, Deserialize)]
pub struct IdentityResolveRequest {
    pub name: String, // "vitalik.eth", "conxian.btc"
    pub protocol: String, // "ENS", "BNS", "WorldID"
}

#[derive(Debug, Serialize)]
pub struct IdentityResolveResponse {
    pub address: String,
    pub protocol: String,
    pub proof_of_personhood: bool,
}

/// [NEXUS-ID-01] Identity resolution for social names and PoP.
pub async fn resolve_identity_handler(
    State(_state): State<AppState>,
    Json(payload): Json<IdentityResolveRequest>,
) -> impl IntoResponse {
    tracing::info!("Resolving identity for {} via {}", payload.name, payload.protocol);

    // [STUB] Integrate Web3.bio, ENS, BNS, and World ID APIs.

    let (address, pop) = match payload.protocol.as_str() {
        "ENS" => ("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(), false),
        "BNS" => ("SP1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM".to_string(), false),
        "WorldID" => ("world_id_nullifier_abc123".to_string(), true),
        _ => ("unknown".to_string(), false),
    };

    Json(IdentityResolveResponse {
        address,
        protocol: payload.protocol,
        proof_of_personhood: pop,
    })
}
