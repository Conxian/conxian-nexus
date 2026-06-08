use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Response,
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

const MAINNET_LIKE_TX_ID: &str =
    "0x4d3f94d20d5d31ef15f4f7f0f6c52f1571318dd43259a59e86cdc84e64546a1e";

fn build_router(rgb_mode: RGBRolloutMode, state: Arc<NexusState>) -> axum::Router {
    let config = Arc::new(Config::default_test());
    let storage = Arc::new(
        Storage::new_lazy(
            "postgres://localhost:1/nexus_test?connect_timeout=1",
            "redis://127.0.0.1/",
        )
        .expect("lazy test storage should be constructible"),
    );
    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        rgb_mode,
        HashSet::new(),
    ));
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    app_router(
        storage, state, executor, None, tableland, None, None, config,
    )
}

async fn response_json(response: Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn test_mmr_proof_missing_params_returns_bad_request() {
    let app = build_router(RGBRolloutMode::Disabled, Arc::new(NexusState::new()));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/mmr-proof")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["error"], "provide either `index` or `tx_id`");
}

#[tokio::test]
async fn test_mmr_proof_invalid_tx_id_returns_bad_request() {
    let app = build_router(RGBRolloutMode::Disabled, Arc::new(NexusState::new()));

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
    let body = response_json(response).await;
    assert_eq!(
        body["error"],
        "Invalid tx_id format: expected 0x-prefixed 32-byte hex string (66 chars)"
    );
}

#[tokio::test]
async fn test_mmr_proof_index_takes_precedence_over_tx_id() {
    let app = build_router(RGBRolloutMode::Disabled, Arc::new(NexusState::new()));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/mmr-proof?index=0&tx_id=bad")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["error"], "leaf at index 0 was not found");
}

#[tokio::test]
async fn test_mmr_proof_unknown_mainnet_tx_returns_not_found() {
    let app = build_router(RGBRolloutMode::Disabled, Arc::new(NexusState::new()));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/mmr-proof?tx_id={MAINNET_LIKE_TX_ID}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(
        body["error"],
        "requested transaction was not found in MMR leaves"
    );
}

#[tokio::test]
async fn test_rgb_contract_missing_query_param_returns_bad_request() {
    let app = build_router(RGBRolloutMode::Disabled, Arc::new(NexusState::new()));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/rgb/contract")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
