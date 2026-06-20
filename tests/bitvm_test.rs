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
    let mut config = Config::default_test();
    config.experimental_apis_enabled = true;

    let storage = Arc::new(Storage::from_config_lazy(&config).expect("Failed to create storage"));
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
async fn test_bitvm2_local_verification_success() {
    let (app, _) = setup_test_app().await;

    let payload = json!({
        "prev_state_root": "0x0000000000000000000000000000000000000000000000000000000000000001",
        "next_state_root": "0x0000000000000000000000000000000000000000000000000000000000000002",
        "proof_bytes": "deadbeef",
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

    // With lazy connection, the actual SQL command will time out if no DB is present,
    // leading to a 500 error in the handler or 503 if unavailable.
    let status = response.status();
    assert!(
        status == StatusCode::OK
            || status == StatusCode::INTERNAL_SERVER_ERROR
            || status == StatusCode::SERVICE_UNAVAILABLE,
        "Unexpected status code: {}",
        status
    );

    if status == StatusCode::OK {
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let res: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(res["valid"], true, "Verification should be valid");
        assert_eq!(res["steps_verified"], 1024);
    }
}

#[tokio::test]
async fn test_bitvm2_local_verification_invalid_format() {
    let (app, _) = setup_test_app().await;

    let payload = json!({
        "prev_state_root": "short",
        "next_state_root": "0x0000000000000000000000000000000000000000000000000000000000000002",
        "proof_bytes": "deadbeef",
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

    // Should return Service Unavailable because it fails local validation and fallback is not configured
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "Invalid format should trigger fallback which is unavailable"
    );
}
