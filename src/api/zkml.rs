//! ZKML remains unavailable until it has a separately reviewed circuit and
//! verification-key contract. It must not reuse the BitVM transition profile.

use crate::api::rest::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZkmlVerifyRequest {
    pub proof: String,
    pub input_commitment: String,
    pub model_id: String,
}

#[derive(Debug, Serialize)]
pub struct ZkmlUnavailableResponse {
    pub code: &'static str,
    pub message: &'static str,
}

pub fn zkml_routes() -> Router<AppState> {
    Router::new().route("/verify", post(verify_zkml_handler))
}

pub async fn verify_zkml_handler(
    State(_state): State<AppState>,
    Json(_payload): Json<ZkmlVerifyRequest>,
) -> impl IntoResponse {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ZkmlUnavailableResponse {
            code: "verifier_unavailable",
            message: "ZKML verification is unavailable until a circuit-specific verifier contract is configured",
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::executor::NexusExecutor;
    use crate::state::NexusState;
    use crate::storage::tableland::TablelandAdapter;
    use crate::storage::Storage;
    use std::collections::HashSet;
    use std::sync::Arc;

    #[tokio::test]
    async fn zkml_fails_closed_without_placeholder_key_or_synthetic_metadata() {
        let config = Arc::new(Config::default_test());
        let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
        let state = AppState {
            config,
            storage: storage.clone(),
            nexus_state: Arc::new(NexusState::new()),
            executor: Arc::new(NexusExecutor::new(
                storage.clone(),
                crate::executor::rgb::RGBRolloutMode::Disabled,
                HashSet::new(),
            )),
            oracle: None,
            tableland: Arc::new(TablelandAdapter::new(storage, "test".to_string())),
            kwil: None,
            nostr: None,
            gateway_url: None,
            http_client: reqwest::Client::new(),
        };
        let response = verify_zkml_handler(
            State(state),
            Json(ZkmlVerifyRequest {
                proof: "proof".to_string(),
                input_commitment: "commitment".to_string(),
                model_id: "model".to_string(),
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
