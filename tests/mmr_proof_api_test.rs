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
async fn test_mmr_proof_fails_closed_when_required_sibling_missing() {
    let config = Config::default_test();

    let storage = match Storage::from_config(&config).await {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping test: Database not available");
            return;
        }
    };

    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        conxian_nexus::executor::rgb::RGBRolloutMode::Disabled,
        std::collections::HashSet::new(),
    ));
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    // Manually insert a leaf but NO nodes in DB.
    // This will trigger the INTERNAL_SERVER_ERROR because siblings are missing.
    nexus_state.update_state("tx1", 100);

    let app = app_router(
        storage,
        nexus_state,
        executor,
        None,
        tableland,
        None,
        None,
        Arc::new(Config::default_test()),
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

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
