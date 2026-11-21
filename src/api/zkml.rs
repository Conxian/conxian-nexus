//! [CON-70] ZKML Verification Logic (Guardian: Attestation).
//! Full implementation of ZKML verification for the compliance module.
//! Requirement: Zero Secret Egress (ZSE) compliance.

use crate::api::rest::AppState;
use crate::verification::zkcp::{verify_groth16_statement, ZkcpProofPayload, ZKML_CIRCUIT_ID};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Json;
use axum::Router;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Deserialize)]
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

/// Derive a deterministic attestation identifier from the verified statement.
///
/// A random identifier would let the same proof mint unlimited distinct
/// attestations, so the identifier commits to the circuit, model and input
/// commitment that were actually verified. The `model_id` is included because
/// it is what the verifying key was selected by, and it is not otherwise bound
/// into the public inputs.
fn attestation_id(request: &ZkmlVerifyRequest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ZKML_CIRCUIT_ID.as_bytes());
    hasher.update(b":");
    hasher.update(request.model_id.trim().as_bytes());
    hasher.update(b":");
    hasher.update(request.input_commitment.trim().to_lowercase().as_bytes());
    format!("zkml_attest_{}", hex::encode(hasher.finalize()))
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
        circuit_id: ZKML_CIRCUIT_ID.to_string(),
        hash_commitment: payload.input_commitment.trim().to_string(),
        verifying_key_bytes: vk_bytes,
        proof_bytes,
        public_inputs: vec![commitment_bytes],
    };

    match verify_groth16_statement(ZKML_CIRCUIT_ID, &zkcp_payload) {
        Ok(true) => {
            let attestation_id = attestation_id(&payload);
            (
                StatusCode::OK,
                Json(ZkmlVerifyResponse {
                    valid: true,
                    attestation_id: Some(attestation_id),
                }),
            )
                .into_response()
        }
        // A rejected proof is not a successful verification: answering 200 here
        // let a caller that only inspects the status code treat a forgery as
        // attested. Every failure mode collapses to 422 so the body shape stays
        // constant and no verifier internals leak.
        _ => (
            StatusCode::UNPROCESSABLE_ENTITY,
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

    /// [R3a] A configured-but-invalid proof must fail closed with a non-2xx
    /// status. Returning 200 with `valid:false` let status-only callers treat a
    /// forgery as attested.
    #[tokio::test]
    async fn test_verify_zkml_handler_rejects_proof_with_422() {
        let mut config = Config::default_test();
        // A syntactically valid base64 verifying key that is not a real VK.
        config.zkml_vks.insert(
            "ZKML_VK_B64_TEST_MODEL".to_owned(),
            BASE64.encode([0u8; 64]),
        );
        let response = verify_zkml_handler(
            State(test_state(config)),
            Json(ZkmlVerifyRequest {
                proof: BASE64.encode([0u8; 128]),
                input_commitment: "00".repeat(32),
                model_id: "test-model".to_owned(),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body, serde_json::json!({ "valid": false }));
    }

    /// Build a genuine Groth16 verifying key and proof over `commitment`, plus
    /// the base64 verifier configuration keyed to `model_id`.
    fn real_zkml_fixture(model_id: &str, commitment: [u8; 32]) -> (Config, ZkmlVerifyRequest) {
        use ark_ff::PrimeField;
        use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
        use ark_relations::lc;
        use ark_serialize::CanonicalSerialize;

        #[derive(Clone)]
        struct OnePublicInput {
            value: ark_bn254::Fr,
        }

        impl ConstraintSynthesizer<ark_bn254::Fr> for OnePublicInput {
            fn generate_constraints(
                self,
                cs: ConstraintSystemRef<ark_bn254::Fr>,
            ) -> Result<(), SynthesisError> {
                let witness = cs.new_witness_variable(|| Ok(self.value))?;
                let public = cs.new_input_variable(|| Ok(self.value))?;
                cs.enforce_r1cs_constraint(
                    || lc!() + witness,
                    || lc!() + ark_relations::gr1cs::Variable::One,
                    || lc!() + public,
                )?;
                Ok(())
            }
        }

        let value = ark_bn254::Fr::from_be_bytes_mod_order(&commitment);
        let rng = &mut ark_std::test_rng();
        let params =
            ark_groth16::Groth16::<ark_bn254::Bn254>::generate_random_parameters_with_reduction(
                OnePublicInput {
                    value: ark_bn254::Fr::from(1u64),
                },
                rng,
            )
            .unwrap();
        let proof = ark_groth16::Groth16::<ark_bn254::Bn254>::create_random_proof_with_reduction(
            OnePublicInput { value },
            &params,
            rng,
        )
        .unwrap();

        let mut vk_bytes = Vec::new();
        params.vk.serialize_compressed(&mut vk_bytes).unwrap();
        let mut proof_bytes = Vec::new();
        proof.serialize_compressed(&mut proof_bytes).unwrap();

        let env_key = format!(
            "ZKML_VK_B64_{}",
            model_id.to_uppercase().replace(['-', '.', ' '], "_")
        );
        let mut config = Config::default_test();
        config.zkml_vks.insert(env_key, BASE64.encode(&vk_bytes));

        let request = ZkmlVerifyRequest {
            proof: BASE64.encode(&proof_bytes),
            input_commitment: hex::encode(commitment),
            model_id: model_id.to_owned(),
        };
        (config, request)
    }

    #[tokio::test]
    async fn test_verify_zkml_accepts_real_proof_and_binds_attestation() {
        let commitment = [5u8; 32];
        let (config, request) = real_zkml_fixture("test-model", commitment);
        let expected = attestation_id(&request);

        let response = verify_zkml_handler(State(test_state(config)), Json(request))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["valid"], Value::Bool(true));
        assert_eq!(body["attestation_id"], Value::String(expected));
    }

    #[tokio::test]
    async fn test_verify_zkml_rejects_zkcp_proof_replay() {
        // A proof valid under the ZKCP circuit identifier must not be accepted
        // as ZKML evidence, even with a correctly configured model key.
        let commitment = [5u8; 32];
        let (config, request) = real_zkml_fixture("test-model", commitment);

        // Re-encode the same proof but declare the ZKCP circuit by verifying it
        // through the ZKCP entry point, which must not be reachable via ZKML.
        let zkcp_payload = crate::verification::ZkcpProofPayload {
            circuit_id: crate::verification::ZKCP_CIRCUIT_ID.to_string(),
            hash_commitment: hex::encode(commitment),
            verifying_key_bytes: BASE64
                .decode(&config.zkml_vks["ZKML_VK_B64_TEST_MODEL"])
                .unwrap(),
            proof_bytes: BASE64.decode(&request.proof).unwrap(),
            public_inputs: vec![commitment],
        };
        assert_eq!(
            crate::verification::verify_groth16_statement(
                crate::verification::ZKML_CIRCUIT_ID,
                &zkcp_payload
            ),
            Err(crate::verification::ZkcpError::CircuitMismatch {
                expected: crate::verification::ZKML_CIRCUIT_ID.to_string(),
                found: crate::verification::ZKCP_CIRCUIT_ID.to_string(),
            })
        );
    }

    #[tokio::test]
    async fn test_verify_zkml_rejects_proof_for_a_different_commitment() {
        let commitment = [5u8; 32];
        let (config, mut request) = real_zkml_fixture("test-model", commitment);
        // Claim a different input commitment than the proof was made over.
        request.input_commitment = hex::encode([6u8; 32]);

        let response = verify_zkml_handler(State(test_state(config)), Json(request))
            .await
            .into_response();
        // dev fails closed on an invalid proof (422); the old 200 + valid:false
        // contract let status-only callers treat a forgery as attested.
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body, serde_json::json!({ "valid": false }));
    }

    #[test]
    fn test_attestation_id_is_deterministic_and_statement_bound() {
        let base = ZkmlVerifyRequest {
            proof: "proof".to_owned(),
            input_commitment: "aa".repeat(32),
            model_id: "model-a".to_owned(),
        };
        assert_eq!(attestation_id(&base), attestation_id(&base.clone()));

        let mut other_model = base.clone();
        other_model.model_id = "model-b".to_owned();
        assert_ne!(attestation_id(&base), attestation_id(&other_model));

        let mut other_commitment = base.clone();
        other_commitment.input_commitment = "bb".repeat(32);
        assert_ne!(attestation_id(&base), attestation_id(&other_commitment));

        // The proof itself is not part of the identity, so re-submitting the
        // same statement cannot mint a second distinct attestation.
        let mut same_statement_new_proof = base.clone();
        same_statement_new_proof.proof = "different".to_owned();
        assert_eq!(
            attestation_id(&base),
            attestation_id(&same_statement_new_proof)
        );
    }
}
