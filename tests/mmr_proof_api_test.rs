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

    if storage.run_migrations().await.is_err() {
        eprintln!("Skipping test: Migrations failed");
        return;
    }

    if sqlx::query("DELETE FROM mmr_nodes")
        .execute(&storage.pg_pool)
        .await
        .is_err()
    {
        eprintln!("Skipping test: Could not reset mmr_nodes");
        return;
    }

    let nexus_state = Arc::new(NexusState::new());
    nexus_state.update_state_batch(&["tx1".to_string(), "tx2".to_string()]);

    let leaf_index = nexus_state
        .get_leaf_index("tx1")
        .expect("tx1 should exist in test state");
    let (_, siblings) = nexus_state
        .get_mmr_proof_metadata(leaf_index)
        .expect("metadata should exist for tx1");
    assert!(
        !siblings.is_empty(),
        "test requires at least one MMR sibling to be required"
    );

    let executor = Arc::new(NexusExecutor::new(storage.clone()));
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        "http://localhost:8080".to_string(),
    ));

    let app = app_router(
        storage,
        nexus_state,
        executor,
        None,
        tableland,
        None,
        None,
        true,
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let error_message = payload
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        error_message.contains("missing required MMR sibling"),
        "unexpected error payload: {payload}"
    );
}
