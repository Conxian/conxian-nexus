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
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn test_external_settlement_trigger_unauthorized() {
    let config = Config::default_test();

    let storage = match Storage::from_config(&config).await {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping test: Database not available");
            return;
        }
    };

    if let Err(_) = storage.run_migrations().await {
        eprintln!("Skipping test: Migrations failed");
        return;
    }

    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(storage.clone()));
    let tableland = Arc::new(TablelandAdapter::new(storage.clone(), "http://localhost:8080".to_string()));

    let app = app_router(storage, nexus_state, executor, None, tableland, None, None, true);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/settlement/trigger")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "source": "ISO20022",
                        "external_id": "MSG123",
                        "payload": {"amount": 100.0, "currency": "USD"},
                        "attestation": "INVALID_TEE"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_external_settlement_trigger_success() {
    let config = Config::default_test();

    let storage = match Storage::from_config(&config).await {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping test: Database not available");
            return;
        }
    };

    if let Err(_) = storage.run_migrations().await {
        eprintln!("Skipping test: Migrations failed");
        return;
    }

    // Seed a block to have a reference height
    sqlx::query("INSERT INTO stacks_blocks (hash, height, type, state) VALUES ('block1', 100, 'burn_block', 'hard')")
        .execute(&storage.pg_pool)
        .await
        .ok(); // Ignore if already exists

    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(storage.clone()));
    let tableland = Arc::new(TablelandAdapter::new(storage.clone(), "http://localhost:8080".to_string()));

    let app = app_router(storage, nexus_state, executor, None, tableland, None, None, true);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/settlement/trigger")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "source": "ISO20022",
                        "external_id": "MSG124",
                        "payload": {"amount": 100.0, "currency": "USD"},
                        "attestation": "TEE_SIGNED_HW"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(res["status"], "Active");
    assert!(res["unlock_height"].as_u64().unwrap() >= 144);
    assert!(res["proposal_id"].as_str().unwrap().starts_with("prop_"));
}
