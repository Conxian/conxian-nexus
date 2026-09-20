//! [CON-70] ZKML Verification Logic (Guardian: Attestation).
//! Full implementation of ZKML verification for the compliance module.
//! Requirement: Zero Secret Egress (ZSE) compliance.

use crate::api::rest::AppState;
use crate::verification::zkcp::{ZkcpProofPayload, ZkcpVerifier, ZKCP_CIRCUIT_ID};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Json;
use axum::Router;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ZkmlUnavailableResponse {
    pub code: &'static str,
    pub message: &'static str,
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
            Json(ZkmlVerifyResponse {
                valid: false,
                attestation_id: None,
            }),
        )
            .into_response();
    }

    // Look up circuit-specific verifying key from configuration
    // Environment variables format: ZKML_VK_B64_<SANITIZED_MODEL_ID>
    let env_key = format!(
        "ZKML_VK_B64_{}",
        payload
            .model_id
            .to_uppercase()
            .replace(['-', '.', ' '], "_")
    );

    let vk_b64 = match state.config.zkml_vks.get(&env_key) {
        Some(vk) => vk,
        None => {
            tracing::warn!(
                model_id = %payload.model_id,
                env_key = %env_key,
                "ZKML verification rejected because no circuit-specific verifier is configured"
            );
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ZkmlUnavailableResponse {
                    code: "verifier_unavailable",
                    message:
                        "ZKML verification is unavailable until a circuit-specific verifier contract is configured",
                }),
            )
                .into_response();
        }
    };

    // Decode base64 verifying key
    let vk_bytes = match BASE64.decode(vk_b64.trim()) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!(error = %e, "Failed to decode ZKML verifying key base64");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ZkmlVerifyResponse {
                    valid: false,
                    attestation_id: None,
                }),
            )
                .into_response();
        }
    };

    // Decode base64 proof
    let proof_bytes = match BASE64.decode(payload.proof.trim()) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ZkmlVerifyResponse {
                    valid: false,
                    attestation_id: None,
                }),
            )
                .into_response();
        }
    };

    // Decode hex input commitment
    let commitment_bytes = match hex::decode(payload.input_commitment.trim()) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut array = [0u8; 32];
            array.copy_from_slice(&bytes);
            array
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ZkmlVerifyResponse {
                    valid: false,
                    attestation_id: None,
                }),
            )
                .into_response();
        }
    };

    let zkcp_payload = ZkcpProofPayload {
        circuit_id: ZKCP_CIRCUIT_ID.to_string(),
        hash_commitment: payload.input_commitment.trim().to_string(),
        verifying_key_bytes: vk_bytes,
        proof_bytes,
        public_inputs: vec![commitment_bytes],
    };

    let verifier = ZkcpVerifier::new();
    match verifier.verify_proof(&zkcp_payload) {
        Ok(true) => {
            let attestation_id = format!("zkml_attest_{}", uuid::Uuid::new_v4());
            (
                StatusCode::OK,
                Json(ZkmlVerifyResponse {
                    valid: true,
                    attestation_id: Some(attestation_id),
                }),
            )
                .into_response()
        }
        _ => (
            StatusCode::OK,
            Json(ZkmlVerifyResponse {
                valid: false,
                attestation_id: None,
            }),
        )
            .into_response(),
    }
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
        assert_eq!(
            body,
            serde_json::json!({
                "code": "verifier_unavailable",
                "message": "ZKML verification is unavailable until a circuit-specific verifier contract is configured"
            })
        );
        assert!(body.get("attestation_id").is_none());
    }

    #[tokio::test]
    async fn test_verify_zkml_handler_rejects_invalid_proof_bytes_when_key_configured() {
        let mut config = Config::default_test();
        config.zkml_vks.insert(
            "ZKML_VK_B64_TEST_MODEL".to_owned(),
            "invalid_base64_vk!!!".to_owned(),
        );
        let response = verify_zkml_handler(
            State(test_state(config)),
            Json(ZkmlVerifyRequest {
                proof: "invalid_proof".to_owned(),
                input_commitment: "00".repeat(32),
                model_id: "test-model".to_owned(),
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
