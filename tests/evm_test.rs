use axum::{
    body::Body,
    http::{Request, StatusCode},
};
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
async fn test_evm_receipt_verification_success() {
    let (app, _) = setup_test_app().await;

    let payload = json!({
        "block_hash": "0x0000000000000000000000000000000000000000000000000000000000000001",
        "transaction_index": 0,
        "proof_nodes": ["node1", "node2"],
        "receipt_root": "0x0000000000000000000000000000000000000000000000000000000000000002"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/evm/verify-receipt")
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
    assert!(res["status"].as_str().unwrap().contains("verified"));
}

#[tokio::test]
async fn test_evm_receipt_verification_invalid_format() {
    let (app, _) = setup_test_app().await;

    let payload = json!({
        "block_hash": "short",
        "transaction_index": 0,
        "proof_nodes": [],
        "receipt_root": "0x0000000000000000000000000000000000000000000000000000000000000002"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/evm/verify-receipt")
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
    assert!(res["status"]
        .as_str()
        .unwrap()
        .contains("Invalid block hash"));
}
