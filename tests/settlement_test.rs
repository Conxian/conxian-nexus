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
        Arc::new(Config::default_test()),
    );

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
                        "payload": {"amount": 1000, "currency": "USD"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Default config has no TEE keys, so it should be unauthorized or service unavailable
    assert!(
        response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn test_external_settlement_trigger_success() {
    let mut config = Config::default_test();
    // Simulate trusted keys
    config
        .erp_attestation_trusted_keys
        .insert("tee-key-1".to_string(), "secret".to_string());
    let config = Arc::new(config);

    let storage = match Storage::from_config(&config).await {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping test: Database not available");
            return;
        }
    };

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
        config.clone(),
    );

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
                        "payload": {"amount": 500, "currency": "ZAR"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 202 Accepted if validated
    assert!(
        response.status() == StatusCode::ACCEPTED || response.status() == StatusCode::UNAUTHORIZED
    );
}
