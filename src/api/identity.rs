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
    State(state): State<AppState>,
    Json(payload): Json<IdentityResolveRequest>,
) -> impl IntoResponse {
    tracing::info!(
        "Resolving identity for {} via {}",
        payload.name,
        payload.protocol
    );

    match payload.protocol.as_str() {
        "BNS" => {
            let name = payload.name.trim();
            if name.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(IdentityResolveResponse {
                        address: "".to_string(),
                        protocol: payload.protocol,
                        proof_of_personhood: false,
                    }),
                )
                    .into_response();
            }

            let mut url = state.stacks_api_base.clone();
            url.set_query(None);
            url.set_fragment(None);
            match url.path_segments_mut() {
                Ok(mut segments) => {
                    segments.clear();
                    segments.extend(["v1", "names", name]);
                }
                Err(()) => {
                    tracing::error!(base_url = %state.stacks_api_base, "Stacks API base URL cannot be a base");
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
            }

            let resp = match state.http_client.get(url).send().await {
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
        "ENS" | "WorldID" => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(IdentityResolveResponse {
                address: "".to_string(),
                protocol: payload.protocol,
                proof_of_personhood: false,
            }),
        )
            .into_response(),
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
