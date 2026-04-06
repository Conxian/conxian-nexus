//! [CON-64] Identity Resolution Layer for Conxian Gateway.
//! Resolves decentralized identities (ENS, BNS, WorldID) to Stacks addresses.

use crate::api::rest::AppState;
use axum::{extract::State, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct IdentityResolveRequest {
    pub name: String,
    pub protocol: String, // "ENS", "BNS", "WorldID"
}

#[derive(Debug, Serialize)]
pub struct IdentityResolveResponse {
    pub address: String,
    pub protocol: String,
    pub proof_of_personhood: bool,
}

/// [NEXUS-ID-01] Identity provider resolution.
pub async fn resolve_identity_handler(
    State(_state): State<AppState>,
    Json(payload): Json<IdentityResolveRequest>,
) -> impl IntoResponse {
    tracing::info!(
        "Resolving identity for {} via {}",
        payload.name,
        payload.protocol
    );

    // [STUB] Integrate Web3.bio, ENS, BNS, and World ID APIs.

    let (address, pop) = match payload.protocol.as_str() {
        "ENS" => (
            "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            false,
        ),
        "BNS" => (
            "SPSZXAKV7DWTDZN2601WR31BM51BD3YTQWE97VRM".to_string(), // Align with SAB bootstrap wallet
            false,
        ),
        "WorldID" => ("world_id_nullifier_abc123".to_string(), true),
        _ => ("unknown".to_string(), false),
    };

    Json(IdentityResolveResponse {
        address,
        protocol: payload.protocol,
        proof_of_personhood: pop,
    })
}
