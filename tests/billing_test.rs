use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use conxian_nexus::api::rest::app_router;
use conxian_nexus::config::Config;
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::tableland::TablelandAdapter;
use conxian_nexus::storage::Storage;
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
#[ignore]
async fn test_billing_flow() {
    let (app, _storage) = setup_test_app().await;

    // 1. Generate Key
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/billing/keys")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"org_id": "org1"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let res: Value = serde_json::from_slice(&body).unwrap();
    let api_key = res["api_key"].as_str().unwrap();

    // 2. Use Key
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/status")
                .header("x-api-key", api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
