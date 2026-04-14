use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use conxian_nexus::api::rest::app_router;
use conxian_nexus::config::Config;
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::Storage;
use conxian_nexus::storage::tableland::TablelandAdapter;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

async fn setup_test_app() -> (axum::Router, Arc<Storage>) {
    dotenvy::dotenv().ok();
    let config = Config::default_test();
    let storage = Arc::new(
        Storage::from_config(&config)
            .await
            .expect("Failed to create storage"),
    );
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(storage.clone()));
    let tableland = Arc::new(TablelandAdapter::new(storage.clone(), "http://localhost:8080".to_string()));
    let experimental_apis_enabled = false;

    (
        app_router(
            storage.clone(),
            nexus_state,
            executor,
            None, // OracleService
            tableland,
            None, // Kwil
            None, // Nostr
            experimental_apis_enabled,
        ),
        storage,
    )
}

#[tokio::test]
#[ignore]
async fn test_billing_flow() {
    let (app, _storage) = setup_test_app().await;

    // 1. Generate Key
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/billing/generate-key")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "developer_email": "test@example.com",
                        "project_name": "Test Project"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let res_json: Value = serde_json::from_slice(&body).unwrap();
    let api_key = res_json["api_key"].as_str().unwrap().to_string();

    // 2. Track Signature (Under Limit)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/billing/telemetry/track-signature")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "api_key": api_key,
                        "signature_hash": "0xabc",
                        "timestamp": 123,
                        "hmac": "fake"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // HMAC will fail since we used 'fake', but we check status
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
