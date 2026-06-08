use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use conxian_nexus::api::rest::app_router;
use conxian_nexus::config::Config;
use conxian_nexus::executor::rgb::RGBRolloutMode;
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::tableland::TablelandAdapter;
use conxian_nexus::storage::Storage;
use std::collections::HashSet;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn test_mmr_proof_invalid_tx_id_returns_bad_request() {
    let config = Arc::new(Config::default_test());
    let storage = Arc::new(
        Storage::new_lazy(
            "postgres://localhost:1/nexus_test?connect_timeout=1",
            "redis://127.0.0.1/",
        )
        .expect("lazy test storage should be constructible"),
    );
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

    let app = app_router(
        storage,
        nexus_state,
        executor,
        None,
        tableland,
        None,
        None,
        config,
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/mmr-proof?tx_id=tx1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["error"],
        "Invalid tx_id format: expected 0x-prefixed 32-byte hex string (66 chars)"
    );
}
