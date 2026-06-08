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
use std::collections::HashSet;
use std::sync::Arc;
use tower::ServiceExt;

const MAINNET_LIKE_TX_ID: &str =
    "0x4d3f94d20d5d31ef15f4f7f0f6c52f1571318dd43259a59e86cdc84e64546a1e";

#[tokio::test]
async fn test_mmr_proof_returns_not_found_for_mainnet_like_tx_when_leaf_absent() {
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
                .uri(format!("/v1/mmr-proof?tx_id={MAINNET_LIKE_TX_ID}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
