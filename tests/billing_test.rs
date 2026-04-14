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
            None, // NostrTelemetry
            experimental_apis_enabled,
        ),
        storage,
    )
}

#[tokio::test]
#[ignore]
async fn test_billing_flow() {
    let (app, storage) = setup_test_app().await;

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
                        "organization_id": "test_org",
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
    let api_secret = res_json["api_secret"].as_str().unwrap().to_string();

    // 2. Track Signature (Under Limit)
    let timestamp = 123456789;
    let sig_hash = "0xabc";
    let message = format!("{}:{}", sig_hash, timestamp);

    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    let hmac_val = hex::encode(mac.finalize().into_bytes());

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
                        "signature_hash": sig_hash,
                        "timestamp": timestamp,
                        "hmac": hmac_val
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
    assert_eq!(res_json["status"], "OK");
    assert_eq!(res_json["current_usage"], 1);

    // 3. Manually inflate usage in Redis to exceed limit (50,000)
    let mut conn = storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .unwrap();
    let redis_key = format!("apikey:{}", api_key);
    let _: () = redis::cmd("HSET")
        .arg(&redis_key)
        .arg("usage")
        .arg(50_000)
        .query_async(&mut conn)
        .await
        .unwrap();

    // 4. Track Signature (First time exceeding limit -> Triggers Grace Period)
    let timestamp = 123456790;
    let sig_hash = "0xdef";
    let message = format!("{}:{}", sig_hash, timestamp);
    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    let hmac_val = hex::encode(mac.finalize().into_bytes());

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
                        "signature_hash": sig_hash,
                        "timestamp": timestamp,
                        "hmac": hmac_val
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Status could be OK or PAYMENT_REQUIRED due to 40% efficiency
    let status = response.status();
    assert!(status == StatusCode::OK || status == StatusCode::PAYMENT_REQUIRED);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let res_json: Value = serde_json::from_slice(&body).unwrap();

    let status_str = res_json["status"].as_str().unwrap();
    assert!(status_str == "OK" || status_str == "THROTTLED");

    // 5. Simulate Grace Period Expiry
    let expired_start = chrono::Utc::now().timestamp() - (25 * 60 * 60); // 25 hours ago
    let _: () = redis::cmd("HSET")
        .arg(&redis_key)
        .arg("grace_period_start")
        .arg(expired_start)
        .query_async(&mut conn)
        .await
        .unwrap();

    let timestamp = 123456791;
    let sig_hash = "0xexpired";
    let message = format!("{}:{}", sig_hash, timestamp);
    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    let hmac_val = hex::encode(mac.finalize().into_bytes());

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
                        "signature_hash": sig_hash,
                        "timestamp": timestamp,
                        "hmac": hmac_val
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
