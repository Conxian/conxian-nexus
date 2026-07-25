//! [CON-70] ZKML Verification Logic (Guardian: Attestation).
//! Full implementation of ZKML verification for the compliance module.
//! Requirement: Zero Secret Egress (ZSE) compliance.

use crate::api::rest::AppState;
use axum::routing::post;
use axum::Router;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZkmlVerifyRequest {
    pub proof: String,
    pub input_commitment: String,
    pub model_id: String,
}

#[derive(Debug, Serialize)]
pub struct ZkmlVerifyResponse {
    pub valid: bool,
}

/// [NEXUS-ZK-01] Zero-knowledge machine learning verification.
pub fn zkml_routes() -> Router<AppState> {
    Router::new().route("/verify", post(verify_zkml_handler))
}

pub async fn verify_zkml_handler(
    State(state): State<AppState>,
    Json(payload): Json<ZkmlVerifyRequest>,
) -> impl IntoResponse {
    tracing::info!(
        "Received ZKML Verification request for model {}",
        payload.model_id
    );

    if payload.proof.trim().is_empty()
        || payload.input_commitment.trim().is_empty()
        || payload.model_id.trim().is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(ZkmlVerifyResponse { valid: false }),
        )
            .into_response();
    }

    let vk_env_key = format!(
        "ZKML_VK_B64_{}",
        payload.model_id.replace('-', "_").to_uppercase()
    );

    let Some(vk_b64) = state.config.zkml_vks.get(&vk_env_key) else {
        tracing::warn!(model_id = %payload.model_id, "ZKML verification key is not configured");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ZkmlVerifyResponse { valid: false }),
        )
            .into_response();
    };

    let is_valid = match lib_conxian_core::bitvm2::verify_state_root_bn254_groth16(
        vk_b64,
        &payload.input_commitment,
        &payload.proof,
        None,
    ) {
        Ok(valid) => valid,
        Err(_) => {
            tracing::warn!(model_id = %payload.model_id, "ZKML proof payload was rejected");
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ZkmlVerifyResponse { valid: false }),
            )
                .into_response();
        }
    };

    (
        if is_valid {
            StatusCode::OK
        } else {
            StatusCode::UNPROCESSABLE_ENTITY
        },
        Json(ZkmlVerifyResponse { valid: is_valid }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::rest::AppState;
    use crate::config::Config;
    use crate::executor::NexusExecutor;
    use crate::state::NexusState;
    use crate::storage::tableland::TablelandAdapter;
    use crate::storage::Storage;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use std::collections::HashSet;
    use std::sync::Arc;

    fn test_state(config: Config) -> AppState {
        let config = Arc::new(config);
        let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
        let nexus_state = Arc::new(NexusState::new());
        let executor = Arc::new(NexusExecutor::new(
            storage.clone(),
            crate::executor::rgb::RGBRolloutMode::Disabled,
            HashSet::new(),
        ));
        let tableland = Arc::new(TablelandAdapter::new(storage.clone(), "test".to_string()));

        AppState {
            config,
            storage,
            nexus_state,
            executor,
            oracle: None,
            tableland,
            kwil: None,
            nostr: None,
            gateway_url: None,
            http_client: reqwest::Client::new(),
        }
    }

    #[tokio::test]
    async fn test_verify_zkml_handler_rejects_empty_payload() {
        let state = test_state(Config::default_test());

        let payload = ZkmlVerifyRequest {
            proof: "".to_string(),
            input_commitment: "".to_string(),
            model_id: "".to_string(),
        };

        let response = verify_zkml_handler(State(state), Json(payload))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_verify_zkml_handler_fails_closed_without_model_key() {
        let state = test_state(Config::default_test());
        let response = verify_zkml_handler(
            State(state),
            Json(ZkmlVerifyRequest {
                proof: "proof".to_owned(),
                input_commitment: "commitment".to_owned(),
                model_id: "missing-model".to_owned(),
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body, serde_json::json!({"valid": false}));
        assert!(body.get("attestation_id").is_none());
        assert!(body.get("confidence").is_none());
    }

    #[tokio::test]
    async fn test_verify_zkml_handler_maps_rejected_proof_without_fake_metadata() {
        let mut config = Config::default_test();
        config.zkml_vks.insert(
            "ZKML_VK_B64_TEST_MODEL".to_owned(),
            "not-a-valid-key".to_owned(),
        );
        let response = verify_zkml_handler(
            State(test_state(config)),
            Json(ZkmlVerifyRequest {
                proof: "not-a-valid-proof".to_owned(),
                input_commitment: "not-a-valid-commitment".to_owned(),
                model_id: "test-model".to_owned(),
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body, serde_json::json!({"valid": false}));
    }
}
