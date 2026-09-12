use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::Engine;
use conxian_nexus::api::rest::app_router;
use conxian_nexus::config::Config;
use conxian_nexus::executor::rgb::RGBRolloutMode;
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::tableland::TablelandAdapter;
use conxian_nexus::storage::Storage;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tower::ServiceExt;

async fn setup_test_app() -> (axum::Router, Arc<Storage>) {
    let config = Config::default_test();
    let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        RGBRolloutMode::Disabled,
        HashSet::new(),
    ));
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    (
        app_router(
            storage.clone(),
            nexus_state,
            executor,
            None,
            tableland,
            None,
            None,
            Arc::new(config),
        ),
        storage,
    )
}

#[tokio::test]
async fn test_cosmos_ibc_verification_success() {
    let (app, _) = setup_test_app().await;

    let raw_header = b"tendermint_header_block_height_100_payload_bytes_sample";
    let encoded_header = base64::engine::general_purpose::STANDARD.encode(raw_header);

    let payload = json!({
        "client_id": "07-tendermint-0",
        "header": encoded_header,
        "trusted_height": 100
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cosmos/verify-ibc")
                .header("Content-Type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(res["valid"], true);
    assert_eq!(res["client_id"], "07-tendermint-0");
    assert!(res["trust_level"].as_str().unwrap().contains("NIP-005 Phase 2"));
}

#[tokio::test]
async fn test_cosmos_ibc_verification_invalid_client() {
    let (app, _) = setup_test_app().await;

    let payload = json!({
        "client_id": "invalid",
        "header": "data",
        "trusted_height": 100
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cosmos/verify-ibc")
                .header("Content-Type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(res["valid"], false);
}
