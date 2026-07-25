use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use conxian_nexus::api::rest::app_router;
use conxian_nexus::config::{BitVmVerificationKeyConfig, Config};
use conxian_nexus::executor::bitvm::{
    ArkworksBn254Verifier, BitVMAdapter, BitVMAuditRecord, BitVMAuditSink, BitVMError,
    BitVMTransition, VerificationKeyRegistry,
};
use conxian_nexus::executor::rgb::RGBRolloutMode;
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::tableland::TablelandAdapter;
use conxian_nexus::storage::Storage;
use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tower::ServiceExt;

#[derive(Deserialize)]
struct Fixture {
    registry: Vec<BitVmVerificationKeyConfig>,
    request: BitVMTransition,
    adversarial_wrong_proof: String,
}

struct MemoryAudit;

#[async_trait]
impl BitVMAuditSink for MemoryAudit {
    async fn persist(&self, _record: &BitVMAuditRecord) -> Result<(), BitVMError> {
        Ok(())
    }
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/bitvm_groth16_v1.json")).unwrap()
}

fn setup_test_app(fixture: &Fixture) -> axum::Router {
    let config = Config::default_test();
    let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
    let bitvm = BitVMAdapter::with_components(
        VerificationKeyRegistry::from_config(&fixture.registry).unwrap(),
        Arc::new(ArkworksBn254Verifier),
        Arc::new(MemoryAudit),
    );
    let executor = Arc::new(NexusExecutor::new_with_bitvm_adapter(
        storage.clone(),
        RGBRolloutMode::Disabled,
        HashSet::new(),
        bitvm,
    ));
    app_router(
        storage.clone(),
        Arc::new(NexusState::new()),
        executor,
        None,
        Arc::new(TablelandAdapter::new(
            storage,
            config.tableland_base_url.clone(),
        )),
        None,
        None,
        Arc::new(config),
    )
}

async fn post(app: axum::Router, request: &BitVMTransition) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/v1/bitvm2/verify-state-root")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(request).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn success_response_contains_only_authenticated_contract_fields() {
    let fixture = fixture();
    let response = post(setup_test_app(&fixture), &fixture.request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let object = body.as_object().unwrap();
    assert_eq!(object.len(), 5);
    assert_eq!(body["valid"], true);
    assert_eq!(body["steps_verified"], 4242);
    assert!(object.contains_key("statement_hash"));
    assert!(object.contains_key("circuit_id"));
    assert!(object.contains_key("verification_key_id"));
    assert!(!object.contains_key("confidence"));
    assert!(!object.contains_key("message"));
}

#[tokio::test]
async fn well_formed_rejected_proof_maps_to_422() {
    let mut fixture = fixture();
    fixture.request.proof = fixture.adversarial_wrong_proof.clone();
    let response = post(setup_test_app(&fixture), &fixture.request).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn unknown_key_maps_to_typed_503_after_statement_rebinding() {
    let fixture = fixture();
    let mut request = fixture.request.clone();
    request.verification_key_id = "aa".repeat(32);
    let inputs: [[u8; 32]; 7] = request
        .public_inputs
        .iter()
        .map(|value| hex::decode(value).unwrap().try_into().unwrap())
        .collect::<Vec<[u8; 32]>>()
        .try_into()
        .unwrap();
    let witness = hex::decode(&request.witness_commitment)
        .unwrap()
        .try_into()
        .unwrap();
    request.statement_hash = hex::encode(
        conxian_nexus::executor::bitvm::canonical_statement_hash(
            [0xaa; 32],
            &inputs,
            witness,
            &request.block_context,
        )
        .unwrap(),
    );
    let response = post(setup_test_app(&fixture), &request).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "verification_key_unavailable");
}

#[tokio::test]
async fn malformed_or_unknown_envelope_fields_map_to_typed_400() {
    let fixture = fixture();
    let mut value = serde_json::to_value(&fixture.request).unwrap();
    value.as_object_mut().unwrap().insert(
        "vk_bytes".to_string(),
        Value::String("forbidden".to_string()),
    );
    let response = setup_test_app(&fixture)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bitvm2/verify-state-root")
                .header("content-type", "application/json")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "malformed_request");
}
