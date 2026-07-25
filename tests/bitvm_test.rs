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
async fn test_legacy_bitvm2_route_is_unavailable_without_deserializing_body() {
    let (app, _) = setup_test_app().await;

    // Use placeholder hex for proof and VK
    let payload = json!({
        "prev_state_root": "0x0000000000000000000000000000000000000000000000000000000000000001",
        "next_state_root": "0x0000000000000000000000000000000000000000000000000000000000000002",
        "proof_bytes": "00",
        "vk_bytes": "00",
        "public_inputs": ["00"],
        "trace_id": "test-trace-1"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bitvm2/verify-state-root")
                .header("Content-Type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(res["error"]["code"], "legacy_bitvm_route_deprecated");
}

#[tokio::test]
async fn test_legacy_bitvm2_route_rejects_even_formerly_success_shaped_payloads() {
    let (app, _) = setup_test_app().await;

    let payload = json!({
        "prev_state_root": "short",
        "next_state_root": "0x0000000000000000000000000000000000000000000000000000000000000002",
        "proof_bytes": "00",
        "vk_bytes": "00",
        "public_inputs": [],
        "trace_id": "test-trace-2"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bitvm2/verify-state-root")
                .header("Content-Type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(res["error"]["code"], "legacy_bitvm_route_deprecated");
}
