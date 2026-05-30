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
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn test_rgb_contract_lookup_shadow_mode() {
    let config = Arc::new(Config::default_test());
    let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(storage.clone()));
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
                .uri("/v1/rgb/contract?contract_id=rgb:test123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["contract_id"], "rgb:test123");
    assert_eq!(json["mode"], "shadow");
    assert_eq!(json["status"], "verified");
}

#[tokio::test]
async fn test_rgb_contract_lookup_not_found() {
    let config = Arc::new(Config::default_test());
    let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(storage.clone()));
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
                .uri("/v1/rgb/contract?contract_id=invalid:id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
