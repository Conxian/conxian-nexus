use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use conxian_nexus::api::rest::app_router;
use conxian_nexus::config::Config;
use conxian_nexus::executor::{rgb::RGBRolloutMode, NexusExecutor};
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::tableland::TablelandAdapter;
use conxian_nexus::storage::Storage;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tower::ServiceExt;

async fn build_test_app() -> Option<axum::Router> {
    let config = Arc::new(Config::default_test());

    let storage = match Storage::from_config(&config).await {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping settlement integration tests: database not available");
            return None;
        }
    };

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

    Some(app_router(
        storage,
        nexus_state,
        executor,
        None,
        tableland,
        None,
        None,
        config,
    ))
}

#[tokio::test]
async fn rejects_missing_routing_policy() {
    let app = match build_test_app().await {
        Some(app) => app,
        None => return,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/settlement/trigger")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "source": "ISO20022",
                        "external_id": "MSG-MISSING-POLICY",
                        "payload": {
                            "amount": 1000,
                            "currency": "USD"
                        },
                        "attestation": "TEE_TEST_SIGNATURE"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "PolicyBlocked");
    assert_eq!(json["policy_rejection"]["code"], "missing_routing_policy");
}

#[tokio::test]
async fn rejects_t1_non_ibc_routing_policy() {
    let app = match build_test_app().await {
        Some(app) => app,
        None => return,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/settlement/trigger")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "source": "ISO20022",
                        "external_id": "MSG-T1-NON-IBC",
                        "payload": {
                            "amount": 500,
                            "currency": "USD",
                            "routing_policy": {
                                "system": "Hyperlane",
                                "trust_tier": "T1",
                                "verification_class": "app_defined_multiverifier",
                                "policy_version": "2026-06-01",
                                "evidence_hash": "0xdef456"
                            }
                        },
                        "attestation": "TEE_TEST_SIGNATURE"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "PolicyBlocked");
    assert_eq!(json["policy_rejection"]["code"], "t1_requires_ibc");
}

#[tokio::test]
async fn allows_valid_approved_policy_payload() {
    let app = match build_test_app().await {
        Some(app) => app,
        None => return,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/settlement/trigger")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "source": "ISO20022",
                        "external_id": "MSG-VALID-POLICY",
                        "payload": {
                            "amount": 750,
                            "currency": "USD",
                            "routing_policy": {
                                "system": "IBC",
                                "trust_tier": "T1",
                                "verification_class": "light_client",
                                "policy_version": "2026-06-01",
                                "evidence_hash": "0xabc123",
                                "requested_trust_tier": "T1"
                            }
                        },
                        "attestation": "TEE_TEST_SIGNATURE"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_ne!(status, StatusCode::FORBIDDEN);
    assert_ne!(
        json.get("status").and_then(|v| v.as_str()),
        Some("PolicyBlocked")
    );
}

#[tokio::test]
async fn test_x402_v2_settlement_verification_endpoint() {
    use k256::schnorr::SigningKey;
    use sha2::{Digest, Sha256};

    let config = Arc::new(Config::default_test());
    let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
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

    let signing_key = SigningKey::from_bytes(&[0x07u8; 32].into()).unwrap();
    let verifying_key = signing_key.verifying_key();

    let scheme = "x402";
    let network = "bitcoin_mainnet";
    let amount_sats = 25000;
    let payer = "bc1qpayer_x402_test";
    let payee = "bc1qpayee_x402_test";
    let nonce = "x402_test_nonce_123456789";
    let timestamp = 1750000000;
    let expires_at = 1750003600;

    let canonical_msg = format!(
        "x402:v2:{}:{}:{}:{}:{}:{}:{}",
        scheme, network, amount_sats, payer, payee, nonce, timestamp
    );
    let msg_digest: [u8; 32] = Sha256::digest(canonical_msg.as_bytes()).into();
    let aux_rand = [0x08u8; 32];
    let signature = signing_key.sign_raw(&msg_digest, &aux_rand).unwrap();

    let payload = json!({
        "protocol_id": conxian_nexus::verification::X402_VERIFIER_ID,
        "scheme": scheme,
        "network": network,
        "amount_sats": amount_sats,
        "payer": payer,
        "payee": payee,
        "nonce": nonce,
        "timestamp": timestamp,
        "expires_at": expires_at,
        "public_key": hex::encode(verifying_key.to_bytes()),
        "signature": hex::encode(signature.to_bytes()),
        "payment_hash": null
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/settlement/x402/verify")
                .header("Content-Type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(res["is_valid"], true);
    assert_eq!(res["scheme"], "x402");
    assert_eq!(res["amount_sats"], 25000);
    assert!(res["authorization_token"]
        .as_str()
        .unwrap()
        .starts_with("x402:v2:"));
}
