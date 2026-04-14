//! [CON-64] Identity Resolution Layer for Conxian Gateway.
//! Resolves decentralized identities (ENS, BNS, WorldID) to Stacks addresses.

use crate::api::rest::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use reqwest::StatusCode as ReqwestStatusCode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct BnsNameResponse {
    address: String,
}

#[derive(Debug, Deserialize)]
pub struct IdentityResolveRequest {
    pub organization_id: String,
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
        "Resolving identity for {} via {} (Org: {})",
        payload.name,
        payload.protocol,
        payload.organization_id
    );

    match payload.protocol.as_str() {
        "BNS" => {
            let stacks_api_base = std::env::var("STACKS_NODE_RPC_URL")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://api.mainnet.hiro.so".to_string());
            let url = format!(
                "{}/v1/names/{}",
                stacks_api_base.trim_end_matches('/'),
                payload.name
            );

            let resp = match reqwest::Client::new().get(url).send().await {
                Ok(resp) => resp,
                Err(err) => {
                    tracing::warn!(error = %err, "BNS name lookup failed");
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(IdentityResolveResponse {
                            address: "".to_string(),
                            protocol: payload.protocol,
                            proof_of_personhood: false,
                        }),
                    )
                        .into_response();
                }
            };

            if resp.status() == ReqwestStatusCode::NOT_FOUND {
                return (
                    StatusCode::NOT_FOUND,
                    Json(IdentityResolveResponse {
                        address: "".to_string(),
                        protocol: payload.protocol,
                        proof_of_personhood: false,
                    }),
                )
                    .into_response();
            }

            if !resp.status().is_success() {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(IdentityResolveResponse {
                        address: "".to_string(),
                        protocol: payload.protocol,
                        proof_of_personhood: false,
                    }),
                )
                    .into_response();
            }

            let parsed: BnsNameResponse = match resp.json().await {
                Ok(parsed) => parsed,
                Err(err) => {
                    tracing::warn!(error = %err, "BNS response JSON parse failed");
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(IdentityResolveResponse {
                            address: "".to_string(),
                            protocol: payload.protocol,
                            proof_of_personhood: false,
                        }),
                    )
                        .into_response();
                }
            };

            (
                StatusCode::OK,
                Json(IdentityResolveResponse {
                    address: parsed.address,
                    protocol: payload.protocol,
                    proof_of_personhood: false,
                }),
            )
                .into_response()
        }
        "ENS" => {
            let url = format!("https://api.ensideas.com/v1/resolve/{}", payload.name);
            let resp = match reqwest::Client::new().get(&url).send().await {
                Ok(r) => r,
                Err(err) => {
                    tracing::warn!(error = %err, "ENS resolution failed");
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(IdentityResolveResponse {
                            address: "".to_string(),
                            protocol: payload.protocol,
                            proof_of_personhood: false,
                        }),
                    ).into_response();
                }
            };
            
            if !resp.status().is_success() {
                return (
                    StatusCode::NOT_FOUND,
                    Json(IdentityResolveResponse {
                        address: "".to_string(),
                        protocol: payload.protocol,
                        proof_of_personhood: false,
                    }),
                ).into_response();
            }

            #[derive(Deserialize)]
            struct EnsResponse { address: Option<String> }
            
            let parsed: EnsResponse = match resp.json().await {
                Ok(p) => p,
                Err(_) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(IdentityResolveResponse {
                            address: "".to_string(),
                            protocol: payload.protocol,
                            proof_of_personhood: false,
                        }),
                    ).into_response();
                }
            };
            
            if let Some(addr) = parsed.address {
                (
                    StatusCode::OK,
                    Json(IdentityResolveResponse {
                        address: addr,
                        protocol: payload.protocol,
                        proof_of_personhood: false,
                    }),
                ).into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(IdentityResolveResponse {
                        address: "".to_string(),
                        protocol: payload.protocol,
                        proof_of_personhood: false,
                    }),
                ).into_response()
            }
        }
        "WorldID" => {
            let app_id = std::env::var("WORLDID_APP_ID").unwrap_or_default();
            if app_id.is_empty() {
                tracing::warn!("WORLDID_APP_ID missing. Dropping WorldID request for security.");
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(IdentityResolveResponse {
                        address: "".to_string(),
                        protocol: payload.protocol,
                        proof_of_personhood: false,
                    }),
                ).into_response();
            }
            
            (
                StatusCode::OK,
                Json(IdentityResolveResponse {
                    address: payload.name.clone(),
                    protocol: payload.protocol,
                    proof_of_personhood: true,
                }),
            ).into_response()
        }
        _ => (
            StatusCode::BAD_REQUEST,
            Json(IdentityResolveResponse {
                address: "".to_string(),
                protocol: payload.protocol,
                proof_of_personhood: false,
            }),
        )
            .into_response(),
    }
}
